use crate::{client::FfmpegClient, progress::Progress, task::Task};
use anyhow::{Result, anyhow};
use std::{
    collections::HashMap,
    sync::mpsc::{self, Sender},
    thread::{self, JoinHandle},
};

#[derive(Debug)]
pub enum NotifyEvent {
    Running(usize),
    Progress(usize, Progress),
    Completed(),
    Failed(usize, String),
    Error(String),
}

pub type RegistryMap = HashMap<usize, Box<dyn Task>>;

pub struct Manager {}

impl Manager {
    pub fn run_serially(tasks: RegistryMap, sender: Sender<NotifyEvent>) -> JoinHandle<()> {
        thread::spawn(move || {
            for (id, task) in tasks {
                let _ = sender.send(NotifyEvent::Running(id));

                let result: Result<()>;
                let ffmpeg_client = Box::new(FfmpegClient::new());

                if task.supports_progress() {
                    let (progress_sender, progress_receiver) = mpsc::channel::<Progress>();
                    result = thread::scope(|s| {
                        let handle = s.spawn(move || {
                            task.execute_with_progress(ffmpeg_client, progress_sender)
                        });
                        while let Ok(p) = progress_receiver.recv() {
                            let _ = sender.send(NotifyEvent::Progress(id, p));
                        }
                        match handle.join() {
                            Ok(task_result) => task_result,
                            Err(panic_box) => {
                                let panic_info = if let Some(msg) = panic_box.downcast_ref::<&str>()
                                {
                                    msg.to_string()
                                } else if let Some(msg) = panic_box.downcast_ref::<String>() {
                                    msg.clone()
                                } else {
                                    format!(
                                        "Unknown panic; unable to resolve exception type: {:?}",
                                        panic_box.type_id()
                                    )
                                };
                                Err(anyhow!(
                                    "A panic occurred in the task thread: {}",
                                    panic_info
                                ))
                            }
                        }
                    });
                } else {
                    result = task.execute(ffmpeg_client);
                }

                let event = match result {
                    Ok(_) => NotifyEvent::Completed(),
                    Err(e) => NotifyEvent::Failed(id, e.to_string()),
                };

                let _ = sender.send(event);
            }
        })
    }
}
