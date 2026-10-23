use std::fmt;

use crate::listing::{ListingRequest, SortField};
use crate::tab::{Tab, TabId};
use crate::workspace::{Navigation, Workspace, WorkspaceError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ActionId(&'static str);

impl ActionId {
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }

    pub fn as_str(self) -> &'static str {
        self.0
    }
}

pub const NAVIGATION_BACK: ActionId = ActionId::new("navigation.back");
pub const NAVIGATION_FORWARD: ActionId = ActionId::new("navigation.forward");
pub const NAVIGATION_PARENT: ActionId = ActionId::new("navigation.parent");
pub const NAVIGATION_BREADCRUMB: ActionId = ActionId::new("navigation.breadcrumb");
pub const RESOURCE_OPEN: ActionId = ActionId::new("resource.open");
pub const TAB_NEW: ActionId = ActionId::new("tab.new");
pub const TAB_CLOSE: ActionId = ActionId::new("tab.close");
pub const TAB_ACTIVATE: ActionId = ActionId::new("tab.activate");
pub const VIEW_SORT_BY_NAME: ActionId = ActionId::new("view.sort_by_name");
pub const VIEW_SORT_BY_SIZE: ActionId = ActionId::new("view.sort_by_size");
pub const VIEW_SORT_BY_MODIFIED: ActionId = ActionId::new("view.sort_by_modified");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Navigation,
    Resources,
    Tabs,
    View,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    BuiltIn,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
}

impl Modifiers {
    pub const NONE: Self = Self {
        control: false,
        alt: false,
        shift: false,
        meta: false,
    };
    pub const CONTROL: Self = Self {
        control: true,
        ..Self::NONE
    };
    pub const ALT: Self = Self {
        alt: true,
        ..Self::NONE
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    Character(char),
    Left,
    Right,
    Up,
    Down,
    Enter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Shortcut {
    pub modifiers: Modifiers,
    pub key: Key,
}

impl Shortcut {
    pub const fn new(modifiers: Modifiers, key: Key) -> Self {
        Self { modifiers, key }
    }
}

const ALT_LEFT: Shortcut = Shortcut::new(Modifiers::ALT, Key::Left);
const ALT_RIGHT: Shortcut = Shortcut::new(Modifiers::ALT, Key::Right);
const ALT_UP: Shortcut = Shortcut::new(Modifiers::ALT, Key::Up);
const ENTER: Shortcut = Shortcut::new(Modifiers::NONE, Key::Enter);
const CONTROL_T: Shortcut = Shortcut::new(Modifiers::CONTROL, Key::Character('t'));
const CONTROL_W: Shortcut = Shortcut::new(Modifiers::CONTROL, Key::Character('w'));

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Target {
    #[default]
    Current,
    Entry(usize),
    Breadcrumb(usize),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Context {
    pub tab: Option<TabId>,
    pub target: Target,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Availability {
    pub visible: bool,
    pub enabled: bool,
}

impl Availability {
    pub const HIDDEN: Self = Self {
        visible: false,
        enabled: false,
    };

    fn shown(enabled: bool) -> Self {
        Self {
            visible: true,
            enabled,
        }
    }
}

#[derive(Debug)]
pub enum Outcome {
    Done,
    Listing(ListingRequest),
    Reordered {
        tab: TabId,
        previous_indices: Vec<usize>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionError {
    Unknown,
    Unavailable,
    Failed(WorkspaceError),
}

impl fmt::Display for ActionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown => formatter.write_str("the action is unknown"),
            Self::Unavailable => formatter.write_str("the action is not available"),
            Self::Failed(error) => fmt::Display::fmt(error, formatter),
        }
    }
}

impl std::error::Error for ActionError {}

type AvailabilityRule = fn(&Workspace, &Context) -> Availability;
type Handler = fn(&mut Workspace, &Context) -> Result<Outcome, WorkspaceError>;

#[derive(Debug, Clone)]
pub struct Action {
    pub id: ActionId,
    pub title: &'static str,
    pub description: &'static str,
    pub category: Category,
    pub keywords: &'static [&'static str],
    pub shortcuts: &'static [Shortcut],
    pub origin: Origin,
    availability: AvailabilityRule,
    handler: Handler,
}

#[derive(Debug, Clone)]
pub struct Registry {
    actions: Vec<Action>,
}

impl Registry {
    pub fn built_in() -> Self {
        Self {
            actions: built_in_actions(),
        }
    }

    pub fn actions(&self) -> &[Action] {
        &self.actions
    }

    pub fn get(&self, id: ActionId) -> Option<&Action> {
        self.actions.iter().find(|action| action.id == id)
    }

    pub fn availability(
        &self,
        id: ActionId,
        workspace: &Workspace,
        context: &Context,
    ) -> Availability {
        match self.get(id) {
            Some(action) => (action.availability)(workspace, context),
            None => Availability::HIDDEN,
        }
    }

    pub fn execute(
        &self,
        id: ActionId,
        workspace: &mut Workspace,
        context: &Context,
    ) -> Result<Outcome, ActionError> {
        let action = self.get(id).ok_or(ActionError::Unknown)?;
        let availability = (action.availability)(workspace, context);
        if !(availability.visible && availability.enabled) {
            return Err(ActionError::Unavailable);
        }
        (action.handler)(workspace, context).map_err(ActionError::Failed)
    }

    pub fn find_shortcut(&self, shortcut: Shortcut) -> Option<ActionId> {
        self.actions
            .iter()
            .find(|action| action.shortcuts.contains(&shortcut))
            .map(|action| action.id)
    }
}

fn built_in_actions() -> Vec<Action> {
    vec![
        Action {
            id: NAVIGATION_BACK,
            title: "Back",
            description: "Go to the previous location in the tab history",
            category: Category::Navigation,
            keywords: &["history", "previous"],
            shortcuts: &[ALT_LEFT],
            origin: Origin::BuiltIn,
            availability: |workspace, context| {
                navigation_availability(workspace, context, Navigation::Back)
            },
            handler: |workspace, context| navigate(workspace, context, Navigation::Back),
        },
        Action {
            id: NAVIGATION_FORWARD,
            title: "Forward",
            description: "Go to the next location in the tab history",
            category: Category::Navigation,
            keywords: &["history", "next"],
            shortcuts: &[ALT_RIGHT],
            origin: Origin::BuiltIn,
            availability: |workspace, context| {
                navigation_availability(workspace, context, Navigation::Forward)
            },
            handler: |workspace, context| navigate(workspace, context, Navigation::Forward),
        },
        Action {
            id: NAVIGATION_PARENT,
            title: "Up",
            description: "Go to the parent folder",
            category: Category::Navigation,
            keywords: &["parent", "up"],
            shortcuts: &[ALT_UP],
            origin: Origin::BuiltIn,
            availability: |workspace, context| {
                navigation_availability(workspace, context, Navigation::Parent)
            },
            handler: |workspace, context| navigate(workspace, context, Navigation::Parent),
        },
        Action {
            id: NAVIGATION_BREADCRUMB,
            title: "Go to breadcrumb",
            description: "Go to the folder of a breadcrumb",
            category: Category::Navigation,
            keywords: &["path", "breadcrumb"],
            shortcuts: &[],
            origin: Origin::BuiltIn,
            availability: |workspace, context| match breadcrumb_navigation(context) {
                Some(navigation) => navigation_availability(workspace, context, navigation),
                None => Availability::HIDDEN,
            },
            handler: |workspace, context| match breadcrumb_navigation(context) {
                Some(navigation) => navigate(workspace, context, navigation),
                None => Err(WorkspaceError::NoSuchLocation),
            },
        },
        Action {
            id: RESOURCE_OPEN,
            title: "Open",
            description: "Open the selected item",
            category: Category::Resources,
            keywords: &["enter", "folder"],
            shortcuts: &[ENTER],
            origin: Origin::BuiltIn,
            availability: |workspace, context| match open_navigation(workspace, context) {
                Some(navigation) => navigation_availability(workspace, context, navigation),
                None => Availability::HIDDEN,
            },
            handler: |workspace, context| match open_navigation(workspace, context) {
                Some(navigation) => navigate(workspace, context, navigation),
                None => Err(WorkspaceError::NoSuchEntry),
            },
        },
        Action {
            id: TAB_NEW,
            title: "New tab",
            description: "Open a new tab in the home folder",
            category: Category::Tabs,
            keywords: &["tab", "home"],
            shortcuts: &[CONTROL_T],
            origin: Origin::BuiltIn,
            availability: |workspace, _| Availability::shown(workspace.home().is_some()),
            handler: |workspace, _| workspace.open_home_tab().map(Outcome::Listing),
        },
        Action {
            id: TAB_CLOSE,
            title: "Close tab",
            description: "Close the tab",
            category: Category::Tabs,
            keywords: &["tab"],
            shortcuts: &[CONTROL_W],
            origin: Origin::BuiltIn,
            availability: |workspace, context| {
                tab_availability(workspace, context, |tab| !tab.pinned())
            },
            handler: |workspace, context| {
                let tab = tab_of(workspace, context).ok_or(WorkspaceError::NoSuchTab)?;
                workspace.close(tab).map(|()| Outcome::Done)
            },
        },
        Action {
            id: TAB_ACTIVATE,
            title: "Switch to tab",
            description: "Make the tab active",
            category: Category::Tabs,
            keywords: &["tab", "switch"],
            shortcuts: &[],
            origin: Origin::BuiltIn,
            availability: |workspace, context| tab_availability(workspace, context, |_| true),
            handler: |workspace, context| {
                let tab = tab_of(workspace, context).ok_or(WorkspaceError::NoSuchTab)?;
                workspace.activate(tab).map(|()| Outcome::Done)
            },
        },
        Action {
            id: VIEW_SORT_BY_NAME,
            title: "Sort by name",
            description: "Sort the folder by name, again to reverse the order",
            category: Category::View,
            keywords: &["sort", "name", "order"],
            shortcuts: &[],
            origin: Origin::BuiltIn,
            availability: sort_availability,
            handler: |workspace, context| sort(workspace, context, SortField::Name),
        },
        Action {
            id: VIEW_SORT_BY_SIZE,
            title: "Sort by size",
            description: "Sort the folder by size, again to reverse the order",
            category: Category::View,
            keywords: &["sort", "size", "order"],
            shortcuts: &[],
            origin: Origin::BuiltIn,
            availability: sort_availability,
            handler: |workspace, context| sort(workspace, context, SortField::Size),
        },
        Action {
            id: VIEW_SORT_BY_MODIFIED,
            title: "Sort by date modified",
            description: "Sort the folder by modification time, again to reverse the order",
            category: Category::View,
            keywords: &["sort", "date", "modified", "time", "order"],
            shortcuts: &[],
            origin: Origin::BuiltIn,
            availability: sort_availability,
            handler: |workspace, context| sort(workspace, context, SortField::Modified),
        },
    ]
}

fn sort_availability(workspace: &Workspace, context: &Context) -> Availability {
    let tab = tab_of(workspace, context).and_then(|id| workspace.tab(id));
    match tab.and_then(Tab::browser) {
        Some(_) => Availability::shown(true),
        None => Availability::HIDDEN,
    }
}

fn sort(
    workspace: &mut Workspace,
    context: &Context,
    field: SortField,
) -> Result<Outcome, WorkspaceError> {
    let tab = tab_of(workspace, context).ok_or(WorkspaceError::NoSuchTab)?;
    let previous_indices = workspace.sort(tab, field)?;
    Ok(Outcome::Reordered {
        tab,
        previous_indices,
    })
}

fn tab_of(workspace: &Workspace, context: &Context) -> Option<TabId> {
    context.tab.or_else(|| workspace.active_id())
}

fn tab_availability(
    workspace: &Workspace,
    context: &Context,
    enabled: impl Fn(&Tab) -> bool,
) -> Availability {
    match tab_of(workspace, context).and_then(|id| workspace.tab(id)) {
        Some(tab) => Availability::shown(enabled(tab)),
        None => Availability::HIDDEN,
    }
}

fn navigation_availability(
    workspace: &Workspace,
    context: &Context,
    navigation: Navigation,
) -> Availability {
    let Some(tab) = tab_of(workspace, context) else {
        return Availability::HIDDEN;
    };
    match workspace.navigation_step(tab, navigation) {
        Ok(_) => Availability::shown(true),
        Err(
            WorkspaceError::NoSuchTab | WorkspaceError::NotABrowser | WorkspaceError::NoSuchEntry,
        ) => Availability::HIDDEN,
        Err(_) => Availability::shown(false),
    }
}

fn navigate(
    workspace: &mut Workspace,
    context: &Context,
    navigation: Navigation,
) -> Result<Outcome, WorkspaceError> {
    let tab = tab_of(workspace, context).ok_or(WorkspaceError::NoSuchTab)?;
    workspace.navigate(tab, navigation).map(Outcome::Listing)
}

fn breadcrumb_navigation(context: &Context) -> Option<Navigation> {
    match context.target {
        Target::Breadcrumb(index) => Some(Navigation::Breadcrumb(index)),
        Target::Current | Target::Entry(_) => None,
    }
}

fn open_navigation(workspace: &Workspace, context: &Context) -> Option<Navigation> {
    match context.target {
        Target::Entry(index) => Some(Navigation::Entry(index)),
        Target::Current => {
            let tab = workspace.tab(tab_of(workspace, context)?)?;
            let selection = tab.browser()?.selection()?;
            Some(Navigation::Entry(selection))
        }
        Target::Breadcrumb(_) => None,
    }
}
