use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::collation::Collation;
use crate::listing::Listed;
use crate::resource::{Location, Metadata, Resource, ResourceKind, ResourceRef};

pub(crate) struct TemporaryDirectory {
    path: PathBuf,
}

impl TemporaryDirectory {
    pub(crate) fn new(name: &str) -> Self {
        let directory = format!("kaze-core-{name}-{}", std::process::id());
        let path = std::env::temp_dir().join(directory);
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create a temporary directory");
        Self { path }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn file(&self, name: &str) -> PathBuf {
        let path = self.path.join(name);
        fs::write(&path, name).expect("create a file");
        path
    }

    pub(crate) fn folder(&self, name: &str) -> PathBuf {
        let path = self.path.join(name);
        fs::create_dir(&path).expect("create a folder");
        path
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::set_permissions(&self.path, fs::Permissions::from_mode(0o700));
        let _ = fs::remove_dir_all(&self.path);
    }
}

pub(crate) fn location(path: &str) -> Location {
    Location::new(ResourceRef::Local(path.into()))
}

pub(crate) fn resource(path: &str, kind: ResourceKind) -> Listed {
    let reference = ResourceRef::Local(path.into());
    let resource = Resource {
        name: reference.name(),
        reference,
        kind,
        link: None,
        hidden: false,
        metadata: Metadata::default(),
    };
    Listed::new(resource, &Collation::default())
}
