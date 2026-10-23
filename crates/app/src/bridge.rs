use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Display;
use std::io::{self, Write};
use std::rc::Rc;
use std::thread;

use chrono::Local;
use kaze_core::action::{
    ActionError, ActionId, Context, NAVIGATION_BACK, NAVIGATION_FORWARD, NAVIGATION_PARENT,
    Outcome, Registry, Target,
};
use kaze_core::collation::Collation;
use kaze_core::listing::{Listed, ListingId, ListingRequest, ListingSummary};
use kaze_core::local;
use kaze_core::resource::{self, ErrorKind, ResourceError};
use kaze_core::tab::{Browser, Tab, TabId};
use kaze_core::workspace::Workspace;
use slint::language::KeyEvent;
use slint::{ComponentHandle, Model, VecModel, Weak};
use tokio::sync::mpsc;

use crate::entries::EntryList;
use crate::{App, Entry, Location, SortColumn, SortDirection, StatusKind, TabMeta};
use crate::{keyboard, present};

pub(crate) const ENTRY_BATCH_SIZE: usize = 64;
const LISTING_BUFFER: usize = 4;

thread_local! {
    pub(crate) static WORKSPACE: RefCell<Workspace> = RefCell::new(Workspace::new());
    pub(crate) static REGISTRY: Registry = Registry::built_in();
    pub(crate) static TAB_TITLES: Rc<VecModel<TabMeta>> = Rc::new(VecModel::default());
    pub(crate) static PROJECTIONS: RefCell<HashMap<TabId, Projection>> =
        RefCell::new(HashMap::new());
}

pub(crate) struct Projection {
    listing: ListingId,
    pub(crate) entries: Rc<EntryList>,
}

pub(crate) fn report_error(context: impl Display, error: impl Display) {
    let _ = writeln!(io::stderr(), "kaze: {context}: {error}");
}

struct Snapshot {
    tabs: Vec<TabMeta>,
    active_tab: i32,
    title: String,
    history: Vec<Location>,
    history_index: i32,
    breadcrumbs: Vec<Location>,
    breadcrumb_index: i32,
    search_placeholder: &'static str,
    entries: Rc<EntryList>,
    selection: i32,
    scroll_position: f32,
    folders: i32,
    files: i32,
    status_text: String,
    status_kind: StatusKind,
    sort_column: SortColumn,
    sort_direction: SortDirection,
    can_back: bool,
    can_forward: bool,
    can_up: bool,
}

impl Snapshot {
    fn collect(
        workspace: &Workspace,
        registry: &Registry,
        projections: &HashMap<TabId, Projection>,
    ) -> Self {
        let tabs: Vec<TabMeta> = workspace.tabs().iter().map(present::tab_meta).collect();
        let active_index = workspace.active_index();
        let title = active_index
            .and_then(|index| tabs.get(index))
            .map(|meta| meta.title.to_string())
            .unwrap_or_default();
        let active = workspace.active_tab();
        let browser = active.and_then(Tab::browser);
        let entries = active
            .and_then(|tab| {
                let listing = tab.browser()?.listing().id();
                let projection = projections.get(&tab.id())?;
                (projection.listing == listing).then(|| projection.entries.clone())
            })
            .unwrap_or_default();
        let locations = |locations: &[resource::Location]| -> Vec<Location> {
            locations.iter().map(present::location).collect()
        };
        let counts = browser
            .map(|browser| browser.listing().counts())
            .unwrap_or_default();
        let (status_text, status_kind) = listing_status(browser);
        let sorting = browser.map(Browser::sorting).unwrap_or_default();
        let enabled = |id| {
            registry
                .availability(id, workspace, &Context::default())
                .enabled
        };
        Self {
            tabs,
            active_tab: present::index(active_index),
            title,
            history: browser.map_or_else(Vec::new, |browser| locations(browser.history())),
            history_index: present::index(browser.map(Browser::history_index)),
            breadcrumbs: browser.map_or_else(Vec::new, |browser| locations(browser.trail())),
            breadcrumb_index: present::index(browser.map(Browser::trail_index)),
            search_placeholder: if browser.is_some() { "Search..." } else { "" },
            entries,
            selection: present::index(browser.and_then(Browser::selection)),
            scroll_position: browser.map_or(0.0, Browser::scroll_offset),
            folders: present::count(counts.folders),
            files: present::count(counts.files),
            status_text,
            status_kind,
            sort_column: present::sort_column(sorting.field),
            sort_direction: present::sort_direction(sorting.direction),
            can_back: enabled(NAVIGATION_BACK),
            can_forward: enabled(NAVIGATION_FORWARD),
            can_up: enabled(NAVIGATION_PARENT),
        }
    }
}

fn listing_status(browser: Option<&Browser>) -> (String, StatusKind) {
    match browser {
        Some(browser) => {
            let listing = browser.listing();
            present::listing_status(listing.state(), listing.unreadable())
        }
        None => (String::new(), StatusKind::None),
    }
}

fn sync_rows<T: Clone + PartialEq + 'static>(model: &VecModel<T>, rows: Vec<T>) {
    while model.row_count() > rows.len() {
        let last = model.row_count().saturating_sub(1);
        model.remove(last);
    }
    for (index, row) in rows.into_iter().enumerate() {
        match model.row_data(index) {
            Some(current) if current == row => {}
            Some(_) => model.set_row_data(index, row),
            None => model.push(row),
        }
    }
}

impl App {
    pub(crate) fn render(&self) {
        let snapshot = WORKSPACE.with(|workspace| {
            let workspace = workspace.try_borrow().ok()?;
            PROJECTIONS.with(|projections| {
                let projections = projections.try_borrow().ok()?;
                let collect =
                    |registry: &Registry| Snapshot::collect(&workspace, registry, &projections);
                Some(REGISTRY.with(collect))
            })
        });
        let Some(snapshot) = snapshot else {
            report_error("cannot show the workspace", "the state is busy");
            return;
        };
        TAB_TITLES.with(|titles| sync_rows(titles, snapshot.tabs));
        self.set_active_tab_index(snapshot.active_tab);
        self.invoke_set_title(snapshot.title.into());
        self.set_history(Rc::new(VecModel::from(snapshot.history)).into());
        self.set_history_index(snapshot.history_index);
        self.set_breadcrumbs(Rc::new(VecModel::from(snapshot.breadcrumbs)).into());
        self.set_breadcrumb_index(snapshot.breadcrumb_index);
        self.set_search_placeholder(snapshot.search_placeholder.into());
        self.set_entries(snapshot.entries.into());
        self.set_active_entry_index(snapshot.selection);
        self.set_scroll_position(snapshot.scroll_position);
        self.set_folder_count(snapshot.folders);
        self.set_file_count(snapshot.files);
        self.set_status_text(snapshot.status_text.into());
        self.set_status_kind(snapshot.status_kind);
        self.set_sort_column(snapshot.sort_column);
        self.set_sort_direction(snapshot.sort_direction);
        self.set_can_back(snapshot.can_back);
        self.set_can_forward(snapshot.can_forward);
        self.set_can_up(snapshot.can_up);
    }

    pub(crate) fn add_overview(&self) {
        WORKSPACE.with(|workspace| match workspace.try_borrow_mut() {
            Ok(mut workspace) => {
                workspace.add_overview();
            }
            Err(error) => report_error("cannot open the overview", error),
        });
        self.render();
    }

    pub(crate) fn set_collation(&self, language: &str) {
        let collation = Collation::new(language);
        WORKSPACE.with(|workspace| match workspace.try_borrow_mut() {
            Ok(mut workspace) => workspace.set_collation(collation),
            Err(error) => report_error("cannot set the sort language", error),
        });
    }

    pub(crate) fn set_home(&self, home: resource::Location) {
        WORKSPACE.with(|workspace| match workspace.try_borrow_mut() {
            Ok(mut workspace) => workspace.set_home(home),
            Err(error) => report_error("cannot remember the home directory", error),
        });
        self.render();
    }

    pub(crate) fn invoke(
        &self,
        id: ActionId,
        context: Context,
    ) -> Result<Option<ListingRequest>, ActionError> {
        let outcome = WORKSPACE.with(|workspace| {
            let Ok(mut workspace) = workspace.try_borrow_mut() else {
                report_error(id.as_str(), "the workspace is busy");
                return Err(ActionError::Unavailable);
            };
            REGISTRY.with(|registry| registry.execute(id, &mut workspace, &context))
        })?;
        let request = match outcome {
            Outcome::Done => None,
            Outcome::Listing(request) => {
                self.project(&request);
                Some(request)
            }
            Outcome::Reordered {
                tab,
                previous_indices,
            } => {
                self.reorder(tab, &previous_indices);
                None
            }
        };
        self.forget_closed_tabs();
        self.render();
        Ok(request)
    }

    pub(crate) fn run_action(&self, id: ActionId, context: Context) -> bool {
        match self.invoke(id, context) {
            Ok(Some(request)) => {
                start_listing(self, request);
                true
            }
            Ok(None) => true,
            Err(_) => false,
        }
    }

    pub(crate) fn tab_context(&self, index: i32) -> Option<Context> {
        let index = usize::try_from(index).ok()?;
        let tab = WORKSPACE.with(|workspace| workspace.try_borrow().ok()?.tab_id_at(index))?;
        Some(Context {
            tab: Some(tab),
            target: Target::Current,
        })
    }

    pub(crate) fn key_pressed(&self, event: &KeyEvent) -> bool {
        let Some(shortcut) = present::shortcut(event, keyboard::pressed_letter()) else {
            return false;
        };
        let Some(id) = REGISTRY.with(|registry| registry.find_shortcut(shortcut)) else {
            return false;
        };
        self.run_action(id, Context::default())
    }

    pub(crate) fn sync_tab(&self, tab_index: i32, entry_index: i32, scroll_position: f32) {
        let Ok(index) = usize::try_from(tab_index) else {
            return;
        };
        let selection = usize::try_from(entry_index).ok();
        let synced = WORKSPACE.with(|workspace| {
            let mut workspace = workspace.try_borrow_mut().ok()?;
            let tab = workspace.tab_id_at(index)?;
            workspace.sync_view(tab, selection, scroll_position).ok()?;
            let browser = workspace.tab(tab)?.browser()?;
            let active = workspace.active_index() == Some(index);
            Some((active, browser.selection(), browser.scroll_offset()))
        });
        if let Some((true, selection, scroll_offset)) = synced {
            self.set_active_entry_index(present::index(selection));
            self.set_scroll_position(scroll_offset);
        }
    }

    pub(crate) fn receive_entries(
        &self,
        tab: TabId,
        listing: ListingId,
        listed: Vec<Listed>,
        entries: Vec<Entry>,
    ) {
        let positions = WORKSPACE.with(|workspace| match workspace.try_borrow_mut() {
            Ok(mut workspace) => workspace.receive(tab, listing, listed),
            Err(error) => {
                report_error("cannot show directory entries", error);
                None
            }
        });
        let Some(positions) = positions else {
            return;
        };
        PROJECTIONS.with(|projections| {
            if let Ok(projections) = projections.try_borrow()
                && let Some(projection) = projections.get(&tab)
                && projection.listing == listing
            {
                projection.entries.insert(&positions, entries);
            }
        });
        self.show_listing();
    }

    pub(crate) fn finish_listing(
        &self,
        tab: TabId,
        listing: ListingId,
        result: Result<ListingSummary, ResourceError>,
    ) {
        WORKSPACE.with(|workspace| match workspace.try_borrow_mut() {
            Ok(mut workspace) => {
                workspace.finish(tab, listing, &result);
            }
            Err(error) => report_error("cannot finish reading a directory", error),
        });
        self.show_listing();
    }

    fn show_listing(&self) {
        let shown = WORKSPACE.with(|workspace| {
            let workspace = workspace.try_borrow().ok()?;
            let browser = workspace.active_tab().and_then(Tab::browser);
            let counts = browser.map(|browser| browser.listing().counts());
            let selection = browser.and_then(Browser::selection);
            let status = listing_status(browser);
            Some((counts.unwrap_or_default(), selection, status))
        });
        let Some((counts, selection, (status_text, status_kind))) = shown else {
            return;
        };
        self.set_active_entry_index(present::index(selection));
        self.set_folder_count(present::count(counts.folders));
        self.set_file_count(present::count(counts.files));
        self.set_status_text(status_text.into());
        self.set_status_kind(status_kind);
    }

    fn reorder(&self, tab: TabId, previous_indices: &[usize]) {
        PROJECTIONS.with(|projections| {
            if let Ok(projections) = projections.try_borrow()
                && let Some(projection) = projections.get(&tab)
            {
                projection.entries.reorder(previous_indices);
            }
        });
    }

    fn project(&self, request: &ListingRequest) {
        let projection = Projection {
            listing: request.listing(),
            entries: Rc::new(EntryList::default()),
        };
        PROJECTIONS.with(|projections| match projections.try_borrow_mut() {
            Ok(mut projections) => {
                projections.insert(request.tab(), projection);
            }
            Err(error) => report_error("cannot prepare directory entries", error),
        });
    }

    fn forget_closed_tabs(&self) {
        let open = WORKSPACE.with(|workspace| {
            let workspace = workspace.try_borrow().ok()?;
            Some(workspace.tabs().iter().map(Tab::id).collect::<Vec<TabId>>())
        });
        let Some(open) = open else {
            return;
        };
        PROJECTIONS.with(|projections| {
            if let Ok(mut projections) = projections.try_borrow_mut() {
                projections.retain(|tab, _| open.contains(tab));
            }
        });
    }
}

enum ListingEvent {
    Batch(Vec<Listed>, Vec<Entry>),
    Finished(Result<ListingSummary, ResourceError>),
}

pub(crate) fn start_listing(app: &App, request: ListingRequest) {
    let (sender, receiver) = mpsc::channel(LISTING_BUFFER);
    let tab = request.tab();
    let listing = request.listing();
    let started = thread::Builder::new()
        .name("kaze-reader".into())
        .spawn(move || read_listing(&request, &sender));
    if let Err(error) = started {
        report_error("cannot start reading a directory", error);
        return;
    }
    let receiving = slint::spawn_local(receive_listing(app.as_weak(), tab, listing, receiver));
    if let Err(error) = receiving {
        report_error("cannot receive directory entries", error);
    }
}

async fn receive_listing(
    app: Weak<App>,
    tab: TabId,
    listing: ListingId,
    mut receiver: mpsc::Receiver<ListingEvent>,
) {
    while let Some(event) = receiver.recv().await {
        let Some(app) = app.upgrade() else {
            return;
        };
        match event {
            ListingEvent::Batch(listed, entries) => {
                app.receive_entries(tab, listing, listed, entries);
            }
            ListingEvent::Finished(result) => app.finish_listing(tab, listing, result),
        }
    }
}

fn read_listing(request: &ListingRequest, sender: &mpsc::Sender<ListingEvent>) {
    let now = Local::now();
    let result = request.run(ENTRY_BATCH_SIZE, |listed| {
        let entries = listed
            .iter()
            .map(|listed| present::entry(listed.resource(), &now))
            .collect();
        let batch = ListingEvent::Batch(listed, entries);
        sender.blocking_send(batch).is_ok()
    });
    report_listing(request, &result);
    let _ = sender.blocking_send(ListingEvent::Finished(result));
}

fn report_listing(request: &ListingRequest, result: &Result<ListingSummary, ResourceError>) {
    let directory = request.resource().display_path();
    match result {
        Ok(summary) => {
            if let Some(first) = summary.unreadable.first() {
                let count = summary.unreadable.len();
                let first_path = first.resource.display_path();
                report_error(
                    format_args!("cannot read {count} entries in {directory}, first {first_path}"),
                    &first.error,
                );
            }
        }
        Err(error) if error.kind() == ErrorKind::Cancelled => {}
        Err(error) => report_error(format_args!("cannot read {directory}"), error),
    }
}

pub(crate) fn resolve_home(app: &App) {
    let app = app.as_weak();
    let resolve = move || match local::home() {
        Ok(home) => {
            let _ = app.upgrade_in_event_loop(move |app| app.set_home(home));
        }
        Err(error) => report_error("cannot find the home directory", error),
    };
    let started = thread::Builder::new()
        .name("kaze-home".into())
        .spawn(resolve);
    if let Err(error) = started {
        report_error("cannot start looking for the home directory", error);
    }
}
