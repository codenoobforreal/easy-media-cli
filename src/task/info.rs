use crate::task::{progress::Progress, status::Status};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Info {
    id: u64,
    name: String,
    file_path: Option<String>,
    file_name: Option<String>,
    status: Status,
    progress: Progress,
    error: Option<String>,
}

impl Info {
    pub fn builder() -> InfoBuilder {
        InfoBuilder::new()
    }
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn file_path(&self) -> Option<&str> {
        self.file_path.as_deref()
    }
    pub fn file_name(&self) -> Option<&str> {
        self.file_name.as_deref()
    }
    pub fn status(&self) -> Status {
        self.status
    }
    pub fn progress(&self) -> Progress {
        self.progress
    }
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn set_id(&mut self, id: u64) {
        self.id = id;
    }

    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }

    pub fn set_file_path(&mut self, path: Option<impl Into<String>>) {
        self.file_path = path.map(Into::into);
    }

    pub fn set_file_name(&mut self, name: Option<impl Into<String>>) {
        self.file_name = name.map(Into::into);
    }

    pub fn set_status(&mut self, status: Status) {
        self.status = status;
    }

    pub fn set_progress(&mut self, progress: Progress) {
        self.progress = progress;
    }

    pub fn set_error(&mut self, err: Option<impl Into<String>>) {
        self.error = err.map(Into::into);
    }
}

#[derive(Debug, Default)]
pub struct InfoBuilder {
    id: u64,
    name: String,
    file_path: Option<String>,
    file_name: Option<String>,
    status: Status,
    progress: Progress,
    error: Option<String>,
}

impl InfoBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn id(mut self, id: u64) -> Self {
        self.id = id;
        self
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    pub fn file_path(mut self, path: Option<impl Into<String>>) -> Self {
        self.file_path = path.map(Into::into);
        self
    }

    pub fn file_name(mut self, name: Option<impl Into<String>>) -> Self {
        self.file_name = name.map(Into::into);
        self
    }

    pub fn status(mut self, status: Status) -> Self {
        self.status = status;
        self
    }

    pub fn progress(mut self, progress: Progress) -> Self {
        self.progress = progress;
        self
    }

    pub fn error(mut self, err: Option<impl Into<String>>) -> Self {
        self.error = err.map(Into::into);
        self
    }

    pub fn build(self) -> Info {
        Info {
            id: self.id,
            name: self.name,
            file_path: self.file_path,
            file_name: self.file_name,
            status: self.status,
            progress: self.progress,
            error: self.error,
        }
    }
}
