use std::fmt;

use crate::collation::Collation;
use crate::listing::{Listed, Listing, ListingId, ListingRequest, ListingSummary, SortField};
use crate::resource::{Location, ResourceError, ResourceRef};
use crate::tab::{Browser, HistoryStep, Tab, TabContent, TabId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceError {
    NoSuchTab,
    NotABrowser,
    NoSuchLocation,
    AlreadyThere,
    NoSuchEntry,
    NotAContainer,
    Pinned,
    HomeUnknown,
}

impl fmt::Display for WorkspaceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::NoSuchTab => "the tab does not exist",
            Self::NotABrowser => "the tab shows no location",
            Self::NoSuchLocation => "there is no such location",
            Self::AlreadyThere => "the location is already open",
            Self::NoSuchEntry => "there is no such entry",
            Self::NotAContainer => "the entry is not a directory",
            Self::Pinned => "the tab is pinned",
            Self::HomeUnknown => "the home directory is unknown",
        };
        formatter.write_str(text)
    }
}

impl std::error::Error for WorkspaceError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Navigation {
    Back,
    Forward,
    Parent,
    Breadcrumb(usize),
    Entry(usize),
}

#[derive(Debug, Default)]
pub struct Workspace {
    tabs: Vec<Tab>,
    active: usize,
    home: Option<Location>,
    collation: Collation,
    next_tab: u64,
    next_listing: u64,
}

impl Workspace {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    pub fn active_index(&self) -> Option<usize> {
        (self.active < self.tabs.len()).then_some(self.active)
    }

    pub fn active_tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active)
    }

    pub fn active_id(&self) -> Option<TabId> {
        self.active_tab().map(Tab::id)
    }

    pub fn tab(&self, id: TabId) -> Option<&Tab> {
        self.tabs.iter().find(|tab| tab.id() == id)
    }

    pub fn tab_id_at(&self, index: usize) -> Option<TabId> {
        self.tabs.get(index).map(Tab::id)
    }

    pub fn home(&self) -> Option<&Location> {
        self.home.as_ref()
    }

    pub fn collation(&self) -> &Collation {
        &self.collation
    }

    pub fn set_collation(&mut self, collation: Collation) {
        self.collation = collation;
    }

    pub fn set_home(&mut self, home: Location) {
        self.home = Some(home);
    }

    pub fn add_overview(&mut self) -> TabId {
        let id = self.next_tab_id();
        self.tabs.push(Tab::new(id, true, TabContent::Overview));
        self.active = self.tabs.len().saturating_sub(1);
        id
    }

    pub fn open_tab(&mut self, location: Location) -> ListingRequest {
        let id = self.next_tab_id();
        let listing = self.next_listing();
        let location = with_known_place(self.home.as_ref(), location);
        let collation = self.collation.clone();
        let request = listing.request(id, location.resource.clone(), collation);
        let home = self.home.clone();
        let browser = Browser::new(location, listing, |resource| {
            locate(home.as_ref(), resource)
        });
        self.tabs
            .push(Tab::new(id, false, TabContent::Browser(browser)));
        self.active = self.tabs.len().saturating_sub(1);
        request
    }

    pub fn open_home_tab(&mut self) -> Result<ListingRequest, WorkspaceError> {
        let home = self.home.clone().ok_or(WorkspaceError::HomeUnknown)?;
        Ok(self.open_tab(home))
    }

    pub fn activate(&mut self, id: TabId) -> Result<(), WorkspaceError> {
        self.active = self.index_of(id)?;
        Ok(())
    }

    pub fn close(&mut self, id: TabId) -> Result<(), WorkspaceError> {
        let index = self.index_of(id)?;
        let tab = self.tabs.get(index).ok_or(WorkspaceError::NoSuchTab)?;
        if tab.pinned() {
            return Err(WorkspaceError::Pinned);
        }
        if let Some(browser) = tab.browser() {
            browser.listing().cancel();
        }
        self.tabs.remove(index);
        if index <= self.active {
            self.active = self.active.saturating_sub(1);
        }
        Ok(())
    }

    pub fn go_to(
        &mut self,
        id: TabId,
        location: Location,
    ) -> Result<ListingRequest, WorkspaceError> {
        let location = with_known_place(self.home.as_ref(), location);
        self.open(id, HistoryStep::Push(location))
    }

    pub fn navigate(
        &mut self,
        id: TabId,
        navigation: Navigation,
    ) -> Result<ListingRequest, WorkspaceError> {
        let step = self.navigation_step(id, navigation)?;
        self.open(id, step)
    }

    pub(crate) fn navigation_step(
        &self,
        id: TabId,
        navigation: Navigation,
    ) -> Result<HistoryStep, WorkspaceError> {
        let browser = self.browser(id)?;
        let history_move = |index: Option<usize>| {
            index
                .filter(|index| *index < browser.history().len())
                .map(HistoryStep::Move)
                .ok_or(WorkspaceError::NoSuchLocation)
        };
        let crumb = |index: Option<usize>| {
            index
                .and_then(|index| browser.trail().get(index))
                .map(|crumb| HistoryStep::Push(crumb.clone()))
                .ok_or(WorkspaceError::NoSuchLocation)
        };
        match navigation {
            Navigation::Back => history_move(browser.history_index().checked_sub(1)),
            Navigation::Forward => history_move(browser.history_index().checked_add(1)),
            Navigation::Parent => crumb(browser.trail_index().checked_sub(1)),
            Navigation::Breadcrumb(index) if index == browser.trail_index() => {
                Err(WorkspaceError::AlreadyThere)
            }
            Navigation::Breadcrumb(index) => crumb(Some(index)),
            Navigation::Entry(index) => {
                let entry = browser
                    .listing()
                    .entries()
                    .get(index)
                    .ok_or(WorkspaceError::NoSuchEntry)?;
                if !entry.kind.is_container() {
                    return Err(WorkspaceError::NotAContainer);
                }
                let location = locate(self.home.as_ref(), entry.reference.clone());
                Ok(HistoryStep::Push(location))
            }
        }
    }

    pub fn receive(
        &mut self,
        id: TabId,
        listing: ListingId,
        batch: Vec<Listed>,
    ) -> Option<Vec<usize>> {
        let browser = self.browser_mut(id).ok()?;
        if browser.listing().id() != listing {
            return None;
        }
        browser.receive(batch)
    }

    pub fn sort(&mut self, id: TabId, field: SortField) -> Result<Vec<usize>, WorkspaceError> {
        Ok(self.browser_mut(id)?.sort(field))
    }

    pub fn finish(
        &mut self,
        id: TabId,
        listing: ListingId,
        result: &Result<ListingSummary, ResourceError>,
    ) -> bool {
        match self.current_listing(id, listing) {
            Some(current) => current.finish(result),
            None => false,
        }
    }

    pub fn sync_view(
        &mut self,
        id: TabId,
        selection: Option<usize>,
        scroll_offset: f32,
    ) -> Result<(), WorkspaceError> {
        self.browser_mut(id)?.sync_view(selection, scroll_offset);
        Ok(())
    }

    fn open(&mut self, id: TabId, step: HistoryStep) -> Result<ListingRequest, WorkspaceError> {
        let listing = self.next_listing();
        let home = self.home.clone();
        let collation = self.collation.clone();
        let browser = self.browser_mut(id)?;
        let opened = browser.open(step, listing, |resource| locate(home.as_ref(), resource));
        let resource = opened
            .map(|location| location.resource.clone())
            .ok_or(WorkspaceError::NoSuchLocation)?;
        Ok(browser.listing().request(id, resource, collation))
    }

    fn current_listing(&mut self, id: TabId, listing: ListingId) -> Option<&mut Listing> {
        let current = self.browser_mut(id).ok()?.listing_mut();
        (current.id() == listing).then_some(current)
    }

    fn browser(&self, id: TabId) -> Result<&Browser, WorkspaceError> {
        let tab = self.tab(id).ok_or(WorkspaceError::NoSuchTab)?;
        tab.browser().ok_or(WorkspaceError::NotABrowser)
    }

    fn browser_mut(&mut self, id: TabId) -> Result<&mut Browser, WorkspaceError> {
        let tab = self
            .tabs
            .iter_mut()
            .find(|tab| tab.id() == id)
            .ok_or(WorkspaceError::NoSuchTab)?;
        tab.browser_mut().ok_or(WorkspaceError::NotABrowser)
    }

    fn index_of(&self, id: TabId) -> Result<usize, WorkspaceError> {
        self.tabs
            .iter()
            .position(|tab| tab.id() == id)
            .ok_or(WorkspaceError::NoSuchTab)
    }

    fn next_tab_id(&mut self) -> TabId {
        let id = TabId(self.next_tab);
        self.next_tab = self.next_tab.wrapping_add(1);
        id
    }

    fn next_listing(&mut self) -> Listing {
        let id = ListingId(self.next_listing);
        self.next_listing = self.next_listing.wrapping_add(1);
        Listing::new(id)
    }
}

fn with_known_place(home: Option<&Location>, location: Location) -> Location {
    match home {
        Some(home) if location.place.is_none() && home.resource == location.resource => {
            home.clone()
        }
        _ => location,
    }
}

fn locate(home: Option<&Location>, resource: ResourceRef) -> Location {
    with_known_place(home, Location::new(resource))
}
