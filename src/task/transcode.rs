// // 缩略图生成任务示例
// #[derive(Debug, Clone)]
// pub struct ThumbnailTask {
//     id: u64,
//     input_path: String,
//     output_path: String,
//     timestamp: String,
// }

// impl ThumbnailTask {
//     pub fn new(id: u64, input_path: String, output_path: String, timestamp: String) -> SharedTask {
//         Arc::new(Self {
//             id,
//             input_path,
//             output_path,
//             timestamp,
//         })
//     }
// }

// #[async_trait]
// impl Task for ThumbnailTask {
//     fn id(&self) -> u64 {
//         self.id
//     }

//     fn name(&self) -> &str {
//         "Thumbnail"
//     }

//     fn file_name(&self) -> Option<&str> {
//         Some(&self.input_path)
//     }

//     async fn run(&self, event_bus: EventBus) -> AppResult<()> {
//         event_bus.publish(crate::event::Event::TaskStarted { task_id: self.id })?;

//         // 缩略图生成通常很快，不需要显示进度
//         let status = Command::new("ffmpeg")
//             .arg("-i")
//             .arg(&self.input_path)
//             .arg("-ss")
//             .arg(&self.timestamp)
//             .arg("-vframes")
//             .arg("1")
//             .arg("-y")
//             .arg(&self.output_path)
//             .stdout(Stdio::null())
//             .stderr(Stdio::null())
//             .status()
//             .await?;

//         if !status.success() {
//             return Err(AppError::FfmpegError(format!(
//                 "Thumbnail generation failed with code: {}",
//                 status.code().unwrap_or(-1)
//             )));
//         }

//         event_bus.publish(crate::event::Event::TaskCompleted { task_id: self.id })?;

//         Ok(())
//     }
// }
