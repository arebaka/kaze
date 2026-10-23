use slint::ToSharedString;

use crate::{Location, File, Device, Node, Cloud, VirtualDevice, LocationType};

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum LocationVariant {
    Other,
    Recent,
    Trash,
    File(File),
    Device(Device),
    Node(Node),
    Cloud(Cloud),
    Virtual(VirtualDevice),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Builder {
    name: String,
    location: LocationVariant,
}

impl Builder {
    pub fn new() -> Self {
        Self {
            name: String::new(),
            location: LocationVariant::Other,
        }
    }

    pub fn name(&mut self, name: String) -> &mut Self {
        self.name = name;
        self
    }
    pub fn name_str(&mut self, name: &str) -> &mut Self {
        self.name = name.into();
        self
    }
    pub fn location(&mut self, location: LocationVariant) -> &mut Self {
        self.location = location;
        self
    }

    pub fn build(&self) -> Location {
        Location {
            name: self.name.to_shared_string(),
            r#type: match self.location {
                LocationVariant::Recent => LocationType::Recent,
                LocationVariant::Trash => LocationType::Trash,
                LocationVariant::File(_) => LocationType::Directory,
                LocationVariant::Device(_) => LocationType::Device,
                LocationVariant::Node(_) => LocationType::Node,
                LocationVariant::Cloud(_) => LocationType::Cloud,
                LocationVariant::Virtual(_) => LocationType::Virtual,
                _ => LocationType::Other,
            },
            file: match self.location.clone() {
                LocationVariant::File(file) => file,
                _ => File::default(),
            },
            device: match self.location.clone() {
                LocationVariant::Device(device) => device,
                _ => Device::default(),
            },
            node: match self.location.clone() {
                LocationVariant::Node(node) => node,
                _ => Node::default(),
            },
            cloud: match self.location.clone() {
                LocationVariant::Cloud(cloud) => cloud,
                _ => Cloud::default(),
            },
            r#virtual: match self.location.clone() {
                LocationVariant::Virtual(device) => device,
                _ => VirtualDevice::default(),
            },
        }
    }
}
