use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Local};
use kaze_core::action::{
    ActionId, Key, Modifiers, Shortcut, VIEW_SORT_BY_MODIFIED, VIEW_SORT_BY_NAME, VIEW_SORT_BY_SIZE,
};
use kaze_core::listing::{self, ListingState, SortField};
use kaze_core::resource::{self, ErrorKind, Place, Resource, ResourceKind, UnixAttributes};
use kaze_core::tab::{Tab, TabContent};
use slint::language::KeyEvent;
use slint::platform::Key as SlintKey;

use crate::directory::Builder as DirectoryBuilder;
use crate::format;
use crate::location::{Builder as LocationBuilder, LocationVariant};
use crate::{
    DirectorySpecial, Entry, File, FileFormat, FileKind, FileType, Location, SortColumn,
    SortDirection, StatusKind, TabMeta, ViewType,
};

pub(crate) fn location(location: &resource::Location) -> Location {
    let special = match location.place {
        Some(Place::Home) => DirectorySpecial::Home,
        None => DirectorySpecial::No,
    };
    let file = File {
        name: location.name.as_str().into(),
        path: location.resource.display_path().into(),
        r#type: FileType::Directory,
        directory: DirectoryBuilder::new().special(special).build(),
        ..Default::default()
    };
    LocationBuilder::new()
        .name(location.name.clone())
        .location(LocationVariant::File(file))
        .build()
}

pub(crate) fn entry(resource: &Resource, now: &DateTime<Local>) -> Entry {
    let metadata = &resource.metadata;
    let access = metadata.access.unwrap_or_default();
    let no_attributes = UnixAttributes::default();
    let unix = metadata.unix.as_ref().unwrap_or(&no_attributes);
    let (created_lo, created_hi) = split_milliseconds(metadata.created);
    let (modified_lo, modified_hi) = split_milliseconds(metadata.modified);
    let (accessed_lo, accessed_hi) = split_milliseconds(metadata.accessed);
    let file = File {
        name: resource.name.as_str().into(),
        path: resource.reference.display_path().into(),
        r#type: file_type(resource.kind),
        link: resource.link.is_some(),
        readable: access.read,
        writable: access.write,
        executable: access.execute,
        owner: unix.user_name.as_deref().unwrap_or_default().into(),
        owner_id: to_int(unix.user_id),
        group: unix.group_name.as_deref().unwrap_or_default().into(),
        group_id: to_int(unix.group_id),
        permissions: to_int(unix.mode),
        inode: to_int(unix.inode),
        hard_links: to_int(unix.hard_links),
        size: to_int(metadata.size.unwrap_or(0)),
        created_lo,
        created_hi,
        modified_lo,
        modified_hi,
        accessed_lo,
        accessed_hi,
        hidden: resource.hidden,
        kind: FileKind::Other,
        format: FileFormat::Other,
        directory: DirectoryBuilder::new().build(),
    };
    let size = match metadata.size {
        Some(bytes) if resource.kind == ResourceKind::Regular => format::file_size(bytes),
        _ => String::new(),
    };
    Entry {
        file,
        size: size.into(),
        created: format::file_time(metadata.created, now).into(),
        modified: format::file_time(metadata.modified, now).into(),
        accessed: format::file_time(metadata.accessed, now).into(),
    }
}

pub(crate) fn tab_meta(tab: &Tab) -> TabMeta {
    match tab.content() {
        TabContent::Overview => TabMeta {
            r#type: ViewType::Overview,
            title: "Overview".into(),
            location: Location::default(),
            permanent: tab.pinned(),
        },
        TabContent::Browser(browser) => {
            let current = browser.location();
            let title = current.map(|current| current.name.as_str());
            TabMeta {
                r#type: ViewType::Table,
                title: title.unwrap_or_default().into(),
                location: current.map(location).unwrap_or_default(),
                permanent: tab.pinned(),
            }
        }
    }
}

pub(crate) fn shortcut(event: &KeyEvent, pressed_letter: Option<char>) -> Option<Shortcut> {
    let mut characters = event.text.chars();
    let character = characters.next()?;
    if characters.next().is_some() {
        return None;
    }
    let key = if character == char::from(SlintKey::LeftArrow) {
        Key::Left
    } else if character == char::from(SlintKey::RightArrow) {
        Key::Right
    } else if character == char::from(SlintKey::UpArrow) {
        Key::Up
    } else if character == char::from(SlintKey::DownArrow) {
        Key::Down
    } else if character == char::from(SlintKey::Return) {
        Key::Enter
    } else {
        let character = character.to_lowercase().next()?;
        let layout_free = match pressed_letter {
            Some(letter) if !character.is_ascii() => letter,
            _ => character,
        };
        Key::Character(layout_free)
    };
    let modifiers = Modifiers {
        control: event.modifiers.control,
        alt: event.modifiers.alt,
        shift: event.modifiers.shift,
        meta: event.modifiers.meta,
    };
    Some(Shortcut::new(modifiers, key))
}

pub(crate) fn index(index: Option<usize>) -> i32 {
    index
        .and_then(|index| i32::try_from(index).ok())
        .unwrap_or(-1)
}

pub(crate) fn count(count: usize) -> i32 {
    i32::try_from(count).unwrap_or(i32::MAX)
}

pub(crate) fn listing_status(state: ListingState, unreadable: usize) -> (String, StatusKind) {
    match state {
        ListingState::Loading => ("Loading...".into(), StatusKind::Progress),
        ListingState::Complete => match unreadable {
            0 => (String::new(), StatusKind::None),
            1 => ("1 unreadable item".into(), StatusKind::Warning),
            count => (format!("{count} unreadable items"), StatusKind::Warning),
        },
        ListingState::Failed(kind) => (failure(kind).into(), StatusKind::Error),
    }
}

pub(crate) fn sort_column(field: SortField) -> SortColumn {
    match field {
        SortField::Name => SortColumn::Name,
        SortField::Size => SortColumn::Size,
        SortField::Modified => SortColumn::Modified,
    }
}

pub(crate) fn sort_direction(direction: listing::SortDirection) -> SortDirection {
    match direction {
        listing::SortDirection::Ascending => SortDirection::Ascending,
        listing::SortDirection::Descending => SortDirection::Descending,
    }
}

pub(crate) fn sort_action(column: SortColumn) -> ActionId {
    match column {
        SortColumn::Name => VIEW_SORT_BY_NAME,
        SortColumn::Size => VIEW_SORT_BY_SIZE,
        SortColumn::Modified => VIEW_SORT_BY_MODIFIED,
    }
}

fn failure(kind: ErrorKind) -> &'static str {
    match kind {
        ErrorKind::NotFound => "Folder not found",
        ErrorKind::PermissionDenied => "Permission denied",
        ErrorKind::NotAContainer => "Not a folder",
        ErrorKind::Unsupported => "Not supported",
        ErrorKind::Cancelled => "Reading cancelled",
        ErrorKind::Other => "Cannot read the folder",
    }
}

fn file_type(kind: ResourceKind) -> FileType {
    match kind {
        ResourceKind::Directory => FileType::Directory,
        ResourceKind::Regular => FileType::Regular,
        ResourceKind::Pipe => FileType::Pipe,
        ResourceKind::BlockDevice => FileType::Block,
        ResourceKind::CharacterDevice => FileType::Character,
        ResourceKind::Socket => FileType::Socket,
        ResourceKind::Other => FileType::Other,
    }
}

fn to_int(value: impl TryInto<i32>) -> i32 {
    value.try_into().unwrap_or(i32::MAX)
}

fn split_milliseconds(time: Option<SystemTime>) -> (i32, i32) {
    let milliseconds = time
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0);
    let low = milliseconds as u32 as i32;
    let high = (milliseconds >> 32) as u32 as i32;
    (low, high)
}
