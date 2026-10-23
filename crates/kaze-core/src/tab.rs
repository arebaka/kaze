use crate::listing::{Listed, Listing, SortField, Sorting};
use crate::resource::{Location, ResourceRef};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TabId(pub(crate) u64);

#[derive(Debug)]
pub enum TabContent {
    Overview,
    Browser(Browser),
}

#[derive(Debug)]
pub struct Tab {
    id: TabId,
    pinned: bool,
    content: TabContent,
}

impl Tab {
    pub(crate) fn new(id: TabId, pinned: bool, content: TabContent) -> Self {
        Self {
            id,
            pinned,
            content,
        }
    }

    pub fn id(&self) -> TabId {
        self.id
    }

    pub fn pinned(&self) -> bool {
        self.pinned
    }

    pub fn content(&self) -> &TabContent {
        &self.content
    }

    pub fn browser(&self) -> Option<&Browser> {
        match &self.content {
            TabContent::Browser(browser) => Some(browser),
            TabContent::Overview => None,
        }
    }

    pub(crate) fn browser_mut(&mut self) -> Option<&mut Browser> {
        match &mut self.content {
            TabContent::Browser(browser) => Some(browser),
            TabContent::Overview => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HistoryStep {
    Push(Location),
    Move(usize),
}

#[derive(Debug)]
pub struct Browser {
    history: Vec<Location>,
    history_index: usize,
    trail: Vec<Location>,
    trail_index: usize,
    listing: Listing,
    sorting: Sorting,
    selection: Option<usize>,
    scroll_offset: f32,
}

impl Browser {
    pub(crate) fn new(
        location: Location,
        listing: Listing,
        locate: impl Fn(ResourceRef) -> Location,
    ) -> Self {
        let (trail, trail_index) = trail(Vec::new(), &location, locate);
        Self {
            history: vec![location],
            history_index: 0,
            trail,
            trail_index,
            listing,
            sorting: Sorting::default(),
            selection: None,
            scroll_offset: 0.0,
        }
    }

    pub fn sorting(&self) -> Sorting {
        self.sorting
    }

    pub fn location(&self) -> Option<&Location> {
        self.history.get(self.history_index)
    }

    pub fn history(&self) -> &[Location] {
        &self.history
    }

    pub fn history_index(&self) -> usize {
        self.history_index
    }

    pub fn trail(&self) -> &[Location] {
        &self.trail
    }

    pub fn trail_index(&self) -> usize {
        self.trail_index
    }

    pub fn listing(&self) -> &Listing {
        &self.listing
    }

    pub(crate) fn listing_mut(&mut self) -> &mut Listing {
        &mut self.listing
    }

    pub fn selection(&self) -> Option<usize> {
        self.selection
    }

    pub fn scroll_offset(&self) -> f32 {
        self.scroll_offset
    }

    pub(crate) fn open(
        &mut self,
        step: HistoryStep,
        listing: Listing,
        locate: impl Fn(ResourceRef) -> Location,
    ) -> Option<&Location> {
        let location = match step {
            HistoryStep::Push(location) => {
                self.history.truncate(self.history_index.saturating_add(1));
                self.history.push(location.clone());
                self.history_index = self.history.len().saturating_sub(1);
                location
            }
            HistoryStep::Move(index) => {
                let location = self.history.get(index)?.clone();
                self.history_index = index;
                location
            }
        };
        let crumbs = std::mem::take(&mut self.trail);
        (self.trail, self.trail_index) = trail(crumbs, &location, locate);
        self.listing.cancel();
        self.listing = listing;
        self.selection = None;
        self.scroll_offset = 0.0;
        self.location()
    }

    pub(crate) fn sort(&mut self, field: SortField) -> Vec<usize> {
        self.sorting = self.sorting.toggled(field);
        let previous_indices = self.listing.resort(self.sorting);
        self.selection = self.selection.and_then(|selected| {
            previous_indices
                .iter()
                .position(|previous| *previous == selected)
        });
        previous_indices
    }

    pub(crate) fn receive(&mut self, batch: Vec<Listed>) -> Option<Vec<usize>> {
        let positions = self.listing.append(batch, self.sorting)?;
        if let Some(selected) = self.selection {
            let mut inserted = positions.clone();
            inserted.sort_unstable();
            let shifted = inserted.into_iter().fold(selected, |index, position| {
                if position <= index {
                    index.saturating_add(1)
                } else {
                    index
                }
            });
            self.selection = Some(shifted);
        }
        Some(positions)
    }

    pub(crate) fn sync_view(&mut self, selection: Option<usize>, scroll_offset: f32) {
        let entries = self.listing.entries().len();
        self.selection = selection.filter(|index| *index < entries);
        self.scroll_offset = scroll_offset;
    }
}

pub(crate) fn trail(
    crumbs: Vec<Location>,
    target: &Location,
    locate: impl Fn(ResourceRef) -> Location,
) -> (Vec<Location>, usize) {
    let same = crumbs
        .iter()
        .position(|crumb| crumb.resource == target.resource);
    if let Some(index) = same {
        return (crumbs, index);
    }
    let mut ancestors = target.resource.ancestors();
    ancestors.pop();
    let mut known = crumbs.into_iter();
    let mut trail: Vec<Location> = ancestors
        .into_iter()
        .map(|ancestor| match known.next() {
            Some(crumb) if crumb.resource == ancestor => crumb,
            _ => locate(ancestor),
        })
        .collect();
    trail.push(target.clone());
    let index = trail.len().saturating_sub(1);
    (trail, index)
}
