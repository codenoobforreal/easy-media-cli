use anyhow::Result;
use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
use easy_media_cli::{
    domain::Event,
    infra::{EventBus, EventHandler},
    task::read_progress,
};
use std::{
    fmt::Write,
    hint::black_box,
    io::Cursor,
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};

struct BenchEventBus {
    call_count: AtomicUsize,
}

impl BenchEventBus {
    fn new() -> Self {
        Self {
            call_count: AtomicUsize::new(0),
        }
    }
}

impl EventBus for BenchEventBus {
    fn publish(&self, event: Event) -> Result<()> {
        self.call_count.fetch_add(1, Ordering::Relaxed);
        black_box(event); // 确保事件对象被实际构造，不被优化
        Ok(())
    }

    fn publish_critical(&self, _event: Event) -> Result<()> {
        unimplemented!()
    }

    fn subscribe(&self, _handler: EventHandler) -> Result<()> {
        unimplemented!()
    }
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation
)]
/// 构造一份高度仿真的 `FFmpeg` 进度输出文本，用于基准测试。
///
/// 该函数生成连续的多帧进度行，模拟一次从 0% 到 100% 的完整编码过程。
/// 生成的数据直接可以作为 `read_progress` 的 `stdout_reader` 输入，
/// 以测量解析器、进度跟踪器及事件发布管道的真实性能。
///
/// # 参数
/// - `total_duration_us`: 视频总时长，单位微秒（μs），决定进度从 0 增长的上限。
/// - `num_lines`: 生成的进度行（帧）总数，控制模拟的帧数量和测试数据规模。
///
/// # 返回
/// 包含完整 `FFmpeg` 进度文本的字节向量（`Vec<u8>`），可直接通过 `Cursor` 转为 `impl Read`。
fn build_realistic_progress(total_duration_us: u64, num_lines: usize) -> Vec<u8> {
    // 预分配一个足够大的 String，避免频繁重新分配内存
    let mut output = String::with_capacity(num_lines * 200);
    let mut frame = 0u64;

    for i in 0..num_lines {
        // 进度比例从 0 线性增长到 1（对应 0% ~ 100%）
        let progress_ratio = i as f64 / (num_lines - 1) as f64;
        // out_time_us 随着进度比例逐步增大，最后一帧达到总时长
        let out_time_us = (total_duration_us as f64 * progress_ratio) as u64;

        // 正常情况下每帧 +1，但大约每 13 帧会额外跳一帧（模拟快速编码或重复帧）
        frame += 1;
        if i % 13 == 0 {
            frame += 1;
        }
        // 注意：真实情况极少出现帧号回退，这里暂不实现，以免引入不必要的解析复杂度

        // 基础速度设为 150x，通过两个不同周期的正弦/余弦波叠加产生波动
        let base_speed = 150.0;
        let noise = (i as f64 * 1.7).sin() * 50.0   // 较大振幅的长周期波动
                  + (i as f64 * 0.3).cos() * 20.0; // 较小振幅的短周期波动
        // 每隔 77 帧模拟一次极慢速度（如 0.5x），代表短暂的系统负载或编解码卡顿
        let speed = if i % 77 == 0 {
            0.5
        } else {
            // 保证速度不低于 0.3x，避免负数或零导致的 eta 计算异常
            (base_speed + noise).max(0.3)
        };

        // fps 与 speed 正相关（通常为 speed 的 0.8 倍 + 基础 10），
        // 再叠加一个独立的波动，以模拟实际中 fps 并不完全跟随 speed 的现象
        let fps = (speed * 0.8 + 10.0 + (i as f64 * 0.7).sin() * 5.0).max(0.1);

        // 只有最后一帧是 'end'，其余都是 'continue'
        let progress_state = if i == num_lines - 1 {
            "end"
        } else {
            "continue"
        };

        let _ = write!(
            output,
            "frame={frame}\n\
             fps={fps:.2}\n\
             stream_0_0_q=2.0\n\
             bitrate=N/A\
             ntotal_size=N/A\
             out_time_us={out_time_us}\n\
             out_time_ms={out_time_us}\n\
             out_time={out_time_formatted}\n\
             dup_frames=0\n\
             drop_frames=0\n\
             speed={speed:4.1}x\n\
             progress={progress_state}\n",
            frame = frame,
            fps = fps,
            out_time_us = out_time_us,
            out_time_formatted = format_duration_us(out_time_us),
            speed = speed,
            progress_state = progress_state,
        );
    }

    output.into_bytes()
}

/// 将微秒数转换为 `FFmpeg` 标准时间格式：`HH:MM:SS.mmmmmm`
///
/// # 参数
/// - `us`: 微秒数（非负整数）
///
/// # 返回
/// 形如 `00:03:04.250733` 的字符串，小时/分钟/秒均为两位数字，微秒为六位数字。
fn format_duration_us(us: u64) -> String {
    let total_secs = us / 1_000_000;
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;
    let micros = us % 1_000_000;
    format!("{hours:02}:{minutes:02}:{seconds:02}.{micros:06}")
}

fn bench_read_progress(c: &mut Criterion) {
    let total_duration = Duration::from_mins(10);
    let total_duration_us = 600_000_000; // 与上面保持一致，10mins
    let num_lines = 500; // 模拟 500 帧进度行

    let progress_data = build_realistic_progress(total_duration_us, num_lines);
    let data_len = progress_data.len() as u64;

    // 让 start_time 足够老，确保首次发布触发
    let start_time = Instant::now()
        .checked_sub(Duration::from_hours(1))
        .unwrap_or(Instant::now());

    let event_bus = BenchEventBus::new();

    let mut group = c.benchmark_group("read_progress");
    group.throughput(Throughput::Bytes(data_len));
    group.bench_function("realistic_stream", |b| {
        b.iter_batched(
            || Cursor::new(progress_data.clone()),
            |cursor| {
                let result = read_progress(
                    1, // task id
                    &event_bus,
                    cursor,
                    start_time,
                    total_duration,
                    Duration::from_millis(100),
                    1.0,
                );
                let _ = black_box(result);
            },
            BatchSize::LargeInput, // 单次迭代处理整个流，计算量较大
        );
    });
    group.finish();
}

criterion_group!(benches, bench_read_progress);
criterion_main!(benches);
