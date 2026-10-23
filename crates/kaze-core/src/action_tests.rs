use std::collections::HashSet;

use crate::action::{
    ActionError, ActionId, Availability, Context, Key, Modifiers, NAVIGATION_BACK,
    NAVIGATION_BREADCRUMB, NAVIGATION_FORWARD, NAVIGATION_PARENT, Origin, Outcome, RESOURCE_OPEN,
    Registry, Shortcut, TAB_ACTIVATE, TAB_CLOSE, TAB_NEW, Target, VIEW_SORT_BY_MODIFIED,
    VIEW_SORT_BY_NAME, VIEW_SORT_BY_SIZE,
};
use crate::listing::{ListingRequest, SortDirection, SortField, Sorting};
use crate::resource::{Place, ResourceKind, ResourceRef};
use crate::tab::Tab;
use crate::testing::{location, resource};
use crate::workspace::Workspace;

const SHOWN: Availability = Availability {
    visible: true,
    enabled: true,
};
const DISABLED: Availability = Availability {
    visible: true,
    enabled: false,
};

fn workspace_at_home() -> (Workspace, ListingRequest) {
    let mut workspace = Workspace::new();
    workspace.add_overview();
    workspace.set_home(location("/home/user").with_place(Place::Home));
    let opened = workspace.open_home_tab().expect("open a home tab");
    (workspace, opened)
}

fn listed(outcome: Outcome) -> ListingRequest {
    match outcome {
        Outcome::Listing(request) => request,
        Outcome::Done | Outcome::Reordered { .. } => panic!("the action lists a folder"),
    }
}

fn local(path: &str) -> ResourceRef {
    ResourceRef::Local(path.into())
}

#[test]
fn built_in_ids_and_shortcuts_are_unique() {
    let registry = Registry::built_in();
    let actions = registry.actions();
    let ids: HashSet<&str> = actions.iter().map(|action| action.id.as_str()).collect();
    assert_eq!(ids.len(), actions.len());
    let shortcuts: Vec<Shortcut> = actions
        .iter()
        .flat_map(|action| action.shortcuts.iter().copied())
        .collect();
    let unique: HashSet<Shortcut> = shortcuts.iter().copied().collect();
    assert_eq!(unique.len(), shortcuts.len());
    for action in actions {
        let id = action.id.as_str();
        assert!(id.contains('.'), "{id} has a namespace");
        assert!(!action.title.is_empty(), "{id} has a title");
        assert!(!action.description.is_empty(), "{id} has a description");
        assert_eq!(action.origin, Origin::BuiltIn);
    }
}

#[test]
fn navigation_availability_follows_the_tab() {
    let registry = Registry::built_in();
    let (workspace, _) = workspace_at_home();
    let state = |id| registry.availability(id, &workspace, &Context::default());
    assert_eq!(state(NAVIGATION_BACK), DISABLED);
    assert_eq!(state(NAVIGATION_FORWARD), DISABLED);
    assert_eq!(state(NAVIGATION_PARENT), SHOWN);
    let crumb = state(NAVIGATION_BREADCRUMB);
    assert_eq!(crumb, Availability::HIDDEN, "needs a breadcrumb");
    let open = state(RESOURCE_OPEN);
    assert_eq!(open, Availability::HIDDEN, "nothing selected");
    assert_eq!(state(TAB_NEW), SHOWN);
    assert_eq!(state(TAB_CLOSE), SHOWN);
}

#[test]
fn navigation_is_hidden_in_the_overview() {
    let registry = Registry::built_in();
    let mut workspace = Workspace::new();
    workspace.add_overview();
    let state = |id| registry.availability(id, &workspace, &Context::default());
    for id in [
        NAVIGATION_BACK,
        NAVIGATION_FORWARD,
        NAVIGATION_PARENT,
        RESOURCE_OPEN,
    ] {
        assert_eq!(state(id), Availability::HIDDEN, "{}", id.as_str());
    }
    assert_eq!(state(TAB_CLOSE), DISABLED, "the overview is pinned");
    assert_eq!(state(TAB_NEW), DISABLED, "the home folder is unknown");
}

#[test]
fn executing_goes_through_availability() {
    let registry = Registry::built_in();
    let (mut workspace, opened) = workspace_at_home();
    let context = Context::default();

    let back = registry.execute(NAVIGATION_BACK, &mut workspace, &context);
    assert_eq!(back.err(), Some(ActionError::Unavailable));
    let unknown = ActionId::new("navigation.nowhere");
    let missing = registry.execute(unknown, &mut workspace, &context);
    assert_eq!(missing.err(), Some(ActionError::Unknown));

    let up = registry.execute(NAVIGATION_PARENT, &mut workspace, &context);
    let request = listed(up.expect("go up"));
    assert_eq!(request.resource(), &local("/home"));
    assert!(opened.cancellation().is_cancelled());
    let back = registry.availability(NAVIGATION_BACK, &workspace, &context);
    assert_eq!(back, SHOWN, "the history has a previous location");
}

#[test]
fn opening_needs_a_folder() {
    let registry = Registry::built_in();
    let (mut workspace, opened) = workspace_at_home();
    let batch = vec![
        resource("/home/user/docs", ResourceKind::Directory),
        resource("/home/user/notes", ResourceKind::Regular),
    ];
    workspace.receive(opened.tab(), opened.listing(), batch);
    let entry = |index| Context {
        tab: None,
        target: Target::Entry(index),
    };

    let notes = registry.availability(RESOURCE_OPEN, &workspace, &entry(1));
    assert_eq!(notes, DISABLED, "files cannot be opened yet");
    workspace
        .sync_view(opened.tab(), Some(0), 0.)
        .expect("select docs");
    let selected = registry.execute(RESOURCE_OPEN, &mut workspace, &Context::default());
    let request = listed(selected.expect("open the selection"));
    assert_eq!(request.resource(), &local("/home/user/docs"));
}

#[test]
fn breadcrumbs_open_their_folder() {
    let registry = Registry::built_in();
    let (mut workspace, _) = workspace_at_home();
    let crumb = |index| Context {
        tab: None,
        target: Target::Breadcrumb(index),
    };

    let current = registry.availability(NAVIGATION_BREADCRUMB, &workspace, &crumb(2));
    assert_eq!(current, DISABLED, "the current crumb");
    let root = registry.execute(NAVIGATION_BREADCRUMB, &mut workspace, &crumb(0));
    let request = listed(root.expect("go to the root"));
    assert_eq!(request.resource(), &local("/"));
    let browser = workspace
        .active_tab()
        .and_then(Tab::browser)
        .expect("the active tab shows a location");
    assert_eq!(browser.trail_index(), 0);
    assert_eq!(browser.trail().len(), 3, "deeper crumbs stay");
}

#[test]
fn tab_actions_use_the_context_tab() {
    let registry = Registry::built_in();
    let (mut workspace, first) = workspace_at_home();
    let second = workspace.open_home_tab().expect("open a second tab");
    let on = |request: &ListingRequest| Context {
        tab: Some(request.tab()),
        target: Target::Current,
    };

    let activated = registry.execute(TAB_ACTIVATE, &mut workspace, &on(&first));
    assert!(matches!(activated, Ok(Outcome::Done)));
    assert_eq!(workspace.active_id(), Some(first.tab()));
    let closed = registry.execute(TAB_CLOSE, &mut workspace, &on(&second));
    assert!(matches!(closed, Ok(Outcome::Done)));
    assert!(workspace.tab(second.tab()).is_none());
    assert!(second.cancellation().is_cancelled());

    let opened = registry.execute(TAB_NEW, &mut workspace, &Context::default());
    let request = listed(opened.expect("open a new tab"));
    assert_eq!(request.resource(), &local("/home/user"));
    assert_eq!(workspace.tabs().len(), 3);
    assert_eq!(workspace.active_id(), Some(request.tab()));
}

#[test]
fn sorting_reorders_and_toggles() {
    let registry = Registry::built_in();
    let (mut workspace, opened) = workspace_at_home();
    let batch = vec![
        resource("/home/user/b", ResourceKind::Regular),
        resource("/home/user/a", ResourceKind::Regular),
        resource("/home/user/c", ResourceKind::Regular),
    ];
    workspace.receive(opened.tab(), opened.listing(), batch);
    let context = Context::default();

    let available = registry.availability(VIEW_SORT_BY_NAME, &workspace, &context);
    assert_eq!(available, SHOWN);
    let reversed = registry.execute(VIEW_SORT_BY_NAME, &mut workspace, &context);
    let Ok(Outcome::Reordered {
        tab,
        previous_indices,
    }) = reversed
    else {
        panic!("sorting reorders the listing");
    };
    assert_eq!(tab, opened.tab());
    assert_eq!(previous_indices, [2, 1, 0]);
    let sorting = |workspace: &Workspace| {
        let browser = workspace.active_tab().and_then(Tab::browser);
        browser.map(|browser| browser.sorting())
    };
    let descending = sorting(&workspace).map(|sorting| sorting.direction);
    assert_eq!(descending, Some(SortDirection::Descending));

    let by_size = registry.execute(VIEW_SORT_BY_SIZE, &mut workspace, &context);
    assert!(matches!(by_size, Ok(Outcome::Reordered { .. })));
    let largest_first = Sorting {
        field: SortField::Size,
        direction: SortDirection::Descending,
    };
    assert_eq!(sorting(&workspace), Some(largest_first));
}

#[test]
fn sorting_is_hidden_in_the_overview() {
    let registry = Registry::built_in();
    let mut workspace = Workspace::new();
    workspace.add_overview();
    let context = Context::default();
    for id in [VIEW_SORT_BY_NAME, VIEW_SORT_BY_SIZE, VIEW_SORT_BY_MODIFIED] {
        let state = registry.availability(id, &workspace, &context);
        assert_eq!(state, Availability::HIDDEN, "{}", id.as_str());
    }
}

#[test]
fn shortcuts_find_their_actions() {
    let registry = Registry::built_in();
    let find = |modifiers, key| registry.find_shortcut(Shortcut::new(modifiers, key));
    assert_eq!(find(Modifiers::ALT, Key::Left), Some(NAVIGATION_BACK));
    assert_eq!(find(Modifiers::ALT, Key::Right), Some(NAVIGATION_FORWARD));
    assert_eq!(find(Modifiers::ALT, Key::Up), Some(NAVIGATION_PARENT));
    assert_eq!(find(Modifiers::NONE, Key::Enter), Some(RESOURCE_OPEN));
    assert_eq!(find(Modifiers::CONTROL, Key::Character('t')), Some(TAB_NEW));
    let control_w = find(Modifiers::CONTROL, Key::Character('w'));
    assert_eq!(control_w, Some(TAB_CLOSE));
    assert_eq!(find(Modifiers::NONE, Key::Left), None);
}
