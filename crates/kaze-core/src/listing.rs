use std::cmp::Ordering;
use std::sync::Arc;
use std::sync::atomic::{self, AtomicBool};

use crate::collation::{Collation, CollationKey};
use crate::local;
use crate::resource::{ErrorKind, Resource, ResourceError, ResourceKind, ResourceRef};
use crate::tab::TabId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ListingId(pub(crate) u64);

#[derive(Debug, Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);

impl Cancellation {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, atomic::Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(atomic::Ordering::Relaxed)
    }
}

#[derive(Debug, Clone)]
pub struct ListingRequest {
    tab: TabId,
    listing: ListingId,
    resource: ResourceRef,
    cancellation: Cancellation,
    collation: Collation,
}

impl ListingRequest {
    pub fn tab(&self) -> TabId {
        self.tab
    }

    pub fn listing(&self) -> ListingId {
        self.listing
    }

    pub fn resource(&self) -> &ResourceRef {
        &self.resource
    }

    pub fn cancellation(&self) -> &Cancellation {
        &self.cancellation
    }

    pub fn run(
        &self,
        batch_size: usize,
        mut deliver: impl FnMut(Vec<Listed>) -> bool,
    ) -> Result<ListingSummary, ResourceError> {
        let collation = &self.collation;
        let keyed = |batch: Vec<Resource>| {
            let listed = batch
                .into_iter()
                .map(|resource| Listed::new(resource, collation));
            deliver(listed.collect())
        };
        match &self.resource {
            ResourceRef::Local(path) => local::list(path, batch_size, &self.cancellation, keyed),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Listed {
    resource: Resource,
    key: CollationKey,
}

impl Listed {
    pub fn new(resource: Resource, collation: &Collation) -> Self {
        let key = collation.key(&resource.name);
        Self { resource, key }
    }

    pub fn resource(&self) -> &Resource {
        &self.resource
    }

    fn row(&self) -> Row<'_> {
        (&self.resource, &self.key)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SortField {
    #[default]
    Name,
    Size,
    Modified,
}

impl SortField {
    pub fn default_direction(self) -> SortDirection {
        match self {
            Self::Name => SortDirection::Ascending,
            Self::Size | Self::Modified => SortDirection::Descending,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SortDirection {
    #[default]
    Ascending,
    Descending,
}

impl SortDirection {
    fn reversed(self) -> Self {
        match self {
            Self::Ascending => Self::Descending,
            Self::Descending => Self::Ascending,
        }
    }

    fn apply(self, ordering: Ordering) -> Ordering {
        match self {
            Self::Ascending => ordering,
            Self::Descending => ordering.reverse(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Sorting {
    pub field: SortField,
    pub direction: SortDirection,
}

impl Sorting {
    pub fn toggled(self, field: SortField) -> Self {
        let direction = if self.field == field {
            self.direction.reversed()
        } else {
            field.default_direction()
        };
        Self { field, direction }
    }
}

#[derive(Debug, Default)]
pub struct ListingSummary {
    pub unreadable: Vec<UnreadableEntry>,
}

#[derive(Debug)]
pub struct UnreadableEntry {
    pub resource: ResourceRef,
    pub error: ResourceError,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListingState {
    Loading,
    Complete,
    Failed(ErrorKind),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EntryCounts {
    pub folders: usize,
    pub files: usize,
}

#[derive(Debug)]
pub struct Listing {
    id: ListingId,
    state: ListingState,
    entries: Vec<Resource>,
    keys: Vec<CollationKey>,
    counts: EntryCounts,
    unreadable: usize,
    cancellation: Cancellation,
}

impl Listing {
    pub(crate) fn new(id: ListingId) -> Self {
        Self {
            id,
            state: ListingState::Loading,
            entries: Vec::new(),
            keys: Vec::new(),
            counts: EntryCounts::default(),
            unreadable: 0,
            cancellation: Cancellation::new(),
        }
    }

    pub fn id(&self) -> ListingId {
        self.id
    }

    pub fn state(&self) -> ListingState {
        self.state
    }

    pub fn entries(&self) -> &[Resource] {
        &self.entries
    }

    pub fn unreadable(&self) -> usize {
        self.unreadable
    }

    pub fn counts(&self) -> EntryCounts {
        self.counts
    }

    pub(crate) fn request(
        &self,
        tab: TabId,
        resource: ResourceRef,
        collation: Collation,
    ) -> ListingRequest {
        ListingRequest {
            tab,
            listing: self.id,
            resource,
            cancellation: self.cancellation.clone(),
            collation,
        }
    }

    pub(crate) fn cancel(&self) {
        self.cancellation.cancel();
    }

    pub(crate) fn append(&mut self, batch: Vec<Listed>, sorting: Sorting) -> Option<Vec<usize>> {
        if self.state != ListingState::Loading {
            return None;
        }
        let mut arrived: Vec<(usize, Listed)> = batch.into_iter().enumerate().collect();
        arrived.sort_by(|(_, left), (_, right)| order(sorting, left.row(), right.row()));
        let entries = std::mem::take(&mut self.entries);
        let keys = std::mem::take(&mut self.keys);
        let splits: Vec<usize> = arrived
            .iter()
            .map(|(_, listed)| split_point(&entries, &keys, sorting, listed.row()))
            .collect();
        let mut positions = vec![0; arrived.len()];
        let mut previous_entries = entries.into_iter();
        let mut previous_keys = keys.into_iter();
        let mut taken = 0;
        for ((arrival, listed), split) in arrived.into_iter().zip(splits) {
            let count = split.saturating_sub(taken);
            self.entries.extend(previous_entries.by_ref().take(count));
            self.keys.extend(previous_keys.by_ref().take(count));
            taken = taken.max(split);
            if let Some(position) = positions.get_mut(arrival) {
                *position = self.entries.len();
            }
            if listed.resource.kind == ResourceKind::Directory {
                self.counts.folders = self.counts.folders.saturating_add(1);
            } else {
                self.counts.files = self.counts.files.saturating_add(1);
            }
            self.entries.push(listed.resource);
            self.keys.push(listed.key);
        }
        self.entries.extend(previous_entries);
        self.keys.extend(previous_keys);
        Some(positions)
    }

    pub(crate) fn resort(&mut self, sorting: Sorting) -> Vec<usize> {
        let entries = std::mem::take(&mut self.entries);
        let keys = std::mem::take(&mut self.keys);
        let mut rows: Vec<(usize, Resource, CollationKey)> = entries
            .into_iter()
            .zip(keys)
            .enumerate()
            .map(|(index, (entry, key))| (index, entry, key))
            .collect();
        rows.sort_by(|left, right| order(sorting, (&left.1, &left.2), (&right.1, &right.2)));
        let mut previous_indices = Vec::new();
        for (index, entry, key) in rows {
            previous_indices.push(index);
            self.entries.push(entry);
            self.keys.push(key);
        }
        previous_indices
    }

    pub(crate) fn finish(&mut self, result: &Result<ListingSummary, ResourceError>) -> bool {
        if self.state != ListingState::Loading {
            return false;
        }
        self.state = match result {
            Ok(summary) => {
                self.unreadable = summary.unreadable.len();
                ListingState::Complete
            }
            Err(error) => ListingState::Failed(error.kind()),
        };
        true
    }
}

type Row<'a> = (&'a Resource, &'a CollationKey);

fn order(sorting: Sorting, left: Row<'_>, right: Row<'_>) -> Ordering {
    let (left_entry, left_key) = left;
    let (right_entry, right_key) = right;
    let left_folder = left_entry.kind == ResourceKind::Directory;
    let right_folder = right_entry.kind == ResourceKind::Directory;
    let direction = sorting.direction;
    let field = match sorting.field {
        SortField::Name => direction.apply(left_key.cmp(right_key)),
        SortField::Size => present_first(
            left_entry.metadata.size,
            right_entry.metadata.size,
            direction,
        ),
        SortField::Modified => present_first(
            left_entry.metadata.modified,
            right_entry.metadata.modified,
            direction,
        ),
    };
    right_folder
        .cmp(&left_folder)
        .then(field)
        .then_with(|| left_key.cmp(right_key))
        .then_with(|| left_entry.name.cmp(&right_entry.name))
}

fn present_first<T: Ord>(left: Option<T>, right: Option<T>, direction: SortDirection) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => direction.apply(left.cmp(&right)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn split_point(
    entries: &[Resource],
    keys: &[CollationKey],
    sorting: Sorting,
    arriving: Row<'_>,
) -> usize {
    let mut low = 0;
    let mut high = entries.len().min(keys.len());
    while low < high {
        let middle = low + (high - low) / 2;
        let goes_first = match (entries.get(middle), keys.get(middle)) {
            (Some(entry), Some(key)) => order(sorting, (entry, key), arriving).is_le(),
            _ => false,
        };
        if goes_first {
            low = middle.saturating_add(1);
        } else {
            high = middle;
        }
    }
    low
}
