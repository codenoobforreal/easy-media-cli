use easy_media_cli::{
    cli::run_cli,
    domain::event::{Event, EventBus},
    infra::DefaultEventBus,
};
use std::{
    process,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

fn main() -> anyhow::Result<()> {
    let event_bus: Arc<dyn EventBus> = Arc::new(DefaultEventBus::default());

    // 全局注册 Ctrl+C 监听：收到信号后向事件总线发布 Shutdown
    let is_first_cancel = Arc::new(AtomicBool::new(true));
    let bus_for_signal = event_bus.clone();
    ctrlc::set_handler(move || {
        if is_first_cancel.load(Ordering::SeqCst) {
            // 第一次按下：优雅取消，等待任务收尾
            is_first_cancel.store(false, Ordering::SeqCst);
            let _ = bus_for_signal.publish(Event::Shutdown);
        } else {
            // 第二次按下：强制退出
            process::exit(1);
        }
    })?;

    run_cli(event_bus)?;
    Ok(())
}
