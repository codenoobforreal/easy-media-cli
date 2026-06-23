//! 业务任务层
//! 逐个具体业务任务的实现
//! 依赖任务编排能力与领域模型，实现具体业务逻辑

mod media_metadata_getter;
mod thumbnail_generator;
mod video_encoder;

pub use media_metadata_getter::MediaMetadataGetter;
pub use thumbnail_generator::ThumbnailGenerator;
pub use video_encoder::VideoEncoder;

#[cfg(test)]
pub mod test_utils {
    use super::*;

    pub use video_encoder::test_utils::make_video_encoder_metadata;
}
