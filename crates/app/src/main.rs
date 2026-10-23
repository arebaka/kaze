mod location;
mod format;
mod directory;
mod present;
mod bridge;
mod entries;
mod keyboard;
mod locale;
#[cfg(test)]
mod entries_tests;
#[cfg(test)]
mod format_tests;
#[cfg(test)]
mod keyboard_tests;
#[cfg(test)]
mod locale_tests;
#[cfg(test)]
mod present_tests;
#[cfg(test)]
mod tab_state_tests;

use kaze_core::action::{
    ActionId, Context, NAVIGATION_BACK, NAVIGATION_BREADCRUMB, NAVIGATION_FORWARD,
    NAVIGATION_PARENT, RESOURCE_OPEN, TAB_ACTIVATE, TAB_CLOSE, TAB_NEW, Target,
};
use slint::ComponentHandle;

use crate::bridge::{TAB_TITLES, resolve_home};

slint::include_modules!();

fn action_handler(app: &App, action: ActionId) -> impl Fn() + 'static {
    let app_weak = app.as_weak();
    move || {
        if let Some(app) = app_weak.upgrade() {
            app.run_action(action, Context::default());
        }
    }
}

pub fn main() -> Result<(), slint::PlatformError> {
    let app = App::new()?;
    keyboard::watch_key_positions(&app);
    app.set_collation(&locale::collation_language());

    TAB_TITLES.with(|tabs| app.set_tabs(tabs.clone().into()));
    app.add_overview();

    let app_weak = app.as_weak();
    app.on_tab_switch(move |index| {
        if let Some(app) = app_weak.upgrade()
            && let Some(context) = app.tab_context(index)
        {
            app.run_action(TAB_ACTIVATE, context);
        }
    });

    let app_weak = app.as_weak();
    app.on_tab_close(move |index| {
        if let Some(app) = app_weak.upgrade()
            && let Some(context) = app.tab_context(index)
        {
            app.run_action(TAB_CLOSE, context);
        }
    });

    app.on_tab_new(action_handler(&app, TAB_NEW));

    let app_weak = app.as_weak();
    app.on_tab_sync(move |tab_index, active_entry_index, scroll_position| {
        if let Some(app) = app_weak.upgrade() {
            app.sync_tab(tab_index, active_entry_index, scroll_position);
        }
    });

    let app_weak = app.as_weak();
    app.on_entry_double_click(move |_, index| {
        if let Some(app) = app_weak.upgrade()
            && let Ok(index) = usize::try_from(index)
        {
            let target = Target::Entry(index);
            app.run_action(RESOURCE_OPEN, Context { tab: None, target });
        }
    });

    app.on_navigate_back(action_handler(&app, NAVIGATION_BACK));
    app.on_navigate_forward(action_handler(&app, NAVIGATION_FORWARD));
    app.on_navigate_up(action_handler(&app, NAVIGATION_PARENT));

    let app_weak = app.as_weak();
    app.on_navigate_breadcrumb(move |index| {
        if let Some(app) = app_weak.upgrade()
            && let Ok(index) = usize::try_from(index)
        {
            let target = Target::Breadcrumb(index);
            app.run_action(NAVIGATION_BREADCRUMB, Context { tab: None, target });
        }
    });

    let app_weak = app.as_weak();
    app.on_sort_by(move |column| {
        if let Some(app) = app_weak.upgrade() {
            app.run_action(present::sort_action(column), Context::default());
        }
    });

    let app_weak = app.as_weak();
    app.on_key_pressed(move |event| {
        app_weak
            .upgrade()
            .is_some_and(|app| app.key_pressed(&event))
    });

    resolve_home(&app);
    app.run()?;
    Ok(())
}
