use crate::task::Type;
use std::{collections::HashMap, ffi::OsString};

pub struct Metadata {
    name: OsString,
    task_type: Type,
    supports_progress: bool,
}

impl Metadata {
    pub fn new(name: OsString, task_type: Type, supports_progress: bool) -> Self {
        Self {
            name,
            task_type,
            supports_progress,
        }
    }

    pub fn name(&self) -> &OsString {
        &self.name
    }

    pub fn task_type(&self) -> Type {
        self.task_type
    }

    pub fn supports_progress(&self) -> bool {
        self.supports_progress
    }
}

pub type MetadataMap = HashMap<usize, Metadata>;
