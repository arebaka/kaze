use std::error::Error;
use std::fmt;
use std::io;
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ResourceRef {
    Local(PathBuf),
}

impl ResourceRef {
    pub fn name(&self) -> String {
        match self {
            Self::Local(path) => path
                .file_name()
                .unwrap_or(path.as_os_str())
                .to_string_lossy()
                .into_owned(),
        }
    }

    pub fn display_path(&self) -> String {
        match self {
            Self::Local(path) => path.to_string_lossy().into_owned(),
        }
    }

    pub fn parent(&self) -> Option<Self> {
        match self {
            Self::Local(path) => path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .map(|parent| Self::Local(parent.to_path_buf())),
        }
    }

    pub fn ancestors(&self) -> Vec<Self> {
        let mut ancestors = vec![self.clone()];
        while let Some(parent) = ancestors.last().and_then(Self::parent) {
            ancestors.push(parent);
        }
        ancestors.reverse();
        ancestors
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    Home,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub resource: ResourceRef,
    pub name: String,
    pub place: Option<Place>,
}

impl Location {
    pub fn new(resource: ResourceRef) -> Self {
        let name = resource.name();
        Self {
            resource,
            name,
            place: None,
        }
    }

    pub fn with_place(self, place: Place) -> Self {
        Self {
            place: Some(place),
            ..self
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
    Directory,
    Regular,
    Pipe,
    BlockDevice,
    CharacterDevice,
    Socket,
    Other,
}

impl ResourceKind {
    pub fn is_container(self) -> bool {
        self == Self::Directory
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    Resolved,
    Broken,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Access {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UnixAttributes {
    pub mode: u32,
    pub user_id: u32,
    pub user_name: Option<String>,
    pub group_id: u32,
    pub group_name: Option<String>,
    pub inode: u64,
    pub hard_links: u64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Metadata {
    pub size: Option<u64>,
    pub created: Option<SystemTime>,
    pub modified: Option<SystemTime>,
    pub accessed: Option<SystemTime>,
    pub access: Option<Access>,
    pub unix: Option<UnixAttributes>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Resource {
    pub reference: ResourceRef,
    pub name: String,
    pub kind: ResourceKind,
    pub link: Option<Link>,
    pub hidden: bool,
    pub metadata: Metadata,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    NotFound,
    PermissionDenied,
    NotAContainer,
    Unsupported,
    Cancelled,
    Other,
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::NotFound => "not found",
            Self::PermissionDenied => "permission denied",
            Self::NotAContainer => "not a directory",
            Self::Unsupported => "not supported",
            Self::Cancelled => "cancelled",
            Self::Other => "failed",
        };
        formatter.write_str(text)
    }
}

#[derive(Debug)]
pub struct ResourceError {
    kind: ErrorKind,
    source: Option<io::Error>,
}

impl ResourceError {
    pub fn new(kind: ErrorKind) -> Self {
        Self { kind, source: None }
    }

    pub fn kind(&self) -> ErrorKind {
        self.kind
    }
}

impl From<io::Error> for ResourceError {
    fn from(error: io::Error) -> Self {
        let kind = match error.kind() {
            io::ErrorKind::NotFound => ErrorKind::NotFound,
            io::ErrorKind::PermissionDenied => ErrorKind::PermissionDenied,
            io::ErrorKind::NotADirectory => ErrorKind::NotAContainer,
            io::ErrorKind::Unsupported => ErrorKind::Unsupported,
            _ => ErrorKind::Other,
        };
        Self {
            kind,
            source: Some(error),
        }
    }
}

impl fmt::Display for ResourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.source {
            Some(source) => fmt::Display::fmt(source, formatter),
            None => fmt::Display::fmt(&self.kind, formatter),
        }
    }
}

impl Error for ResourceError {}
