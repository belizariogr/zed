use crate::{
    Continue, Pause, RerunSession, StepInto, StepOut, StepOver, Stop, debugger_panel::DebugPanel,
    session::running::RunningState,
};
use gpui::{Action, Entity, Focusable, MouseButton, Subscription, WeakEntity};
use project::debugger::session::{SessionStateEvent, ThreadStatus};
use ui::{Divider, Tooltip, prelude::*};

pub(crate) struct DebugControls {
    panel: WeakEntity<DebugPanel>,
    running_state: Option<Entity<RunningState>>,
    _panel_subscription: Subscription,
    session_subscriptions: Vec<Subscription>,
}

impl DebugControls {
    pub(crate) fn new(panel: &Entity<DebugPanel>, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            panel: panel.downgrade(),
            running_state: None,
            _panel_subscription: cx.observe(panel, |this, _, cx| this.update_session(cx)),
            session_subscriptions: Vec::new(),
        };
        this.update_session(cx);
        this
    }

    fn update_session(&mut self, cx: &mut Context<Self>) {
        let running_state = self.panel.upgrade().and_then(|panel| {
            panel
                .read(cx)
                .active_session()
                .map(|session| session.read(cx).running_state().clone())
        });
        if running_state != self.running_state {
            self.session_subscriptions.clear();
            if let Some(running_state) = &running_state {
                let session = running_state.read(cx).session().clone();
                self.session_subscriptions = vec![
                    cx.observe(running_state, |_, _, cx| cx.notify()),
                    cx.observe(&session, |_, _, cx| cx.notify()),
                    cx.subscribe(&session, |_, _, _: &SessionStateEvent, cx| cx.notify()),
                ];
            }
            self.running_state = running_state;
        }
        cx.notify();
    }
}

impl Render for DebugControls {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(running_state) = self
            .running_state
            .as_ref()
            .filter(|state| !state.read(cx).session().read(cx).is_terminated())
        else {
            return div().into_any_element();
        };
        let state = running_state.read(cx);
        let status = state.thread_status(cx);
        let is_stopped = status == Some(ThreadStatus::Stopped);
        let is_running = status == Some(ThreadStatus::Running);
        let focus_handle = state.focus_handle(cx);
        let (icon, label, action): (_, _, Box<dyn Action>) = if is_stopped {
            (
                IconName::DebugContinue,
                "Continue Program",
                Box::new(Continue),
            )
        } else {
            (IconName::DebugPause, "Pause Program", Box::new(Pause))
        };

        h_flex()
            .id("debug-titlebar-controls")
            .debug_selector(|| "debug-titlebar-controls".into())
            .flex_shrink_0()
            .gap_0p5()
            .mr_2()
            .px_1()
            .py_0()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().colors().border)
            .bg(cx.theme().colors().elevated_surface_background)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                IconButton::new("debug-titlebar-continue-pause", icon)
                    .icon_size(IconSize::Small)
                    .disabled(!is_stopped && !is_running)
                    .on_click(window.listener_for(running_state, move |state, _, _, cx| {
                        if is_stopped {
                            state.continue_program(cx);
                        } else {
                            state.pause_thread(cx);
                        }
                    }))
                    .tooltip({
                        let focus_handle = focus_handle.clone();
                        move |_, cx| {
                            Tooltip::for_action_in(label, action.as_ref(), &focus_handle, cx)
                        }
                    }),
            )
            .children(
                [
                    (
                        "debug-titlebar-step-over",
                        IconName::DebugStepOver,
                        "Step Over",
                        Box::new(StepOver) as Box<dyn Action>,
                        RunningState::step_over
                            as fn(&mut RunningState, &mut Context<RunningState>),
                    ),
                    (
                        "debug-titlebar-step-in",
                        IconName::DebugStepInto,
                        "Step In",
                        Box::new(StepInto) as Box<dyn Action>,
                        RunningState::step_in,
                    ),
                    (
                        "debug-titlebar-step-out",
                        IconName::DebugStepOut,
                        "Step Out",
                        Box::new(StepOut) as Box<dyn Action>,
                        RunningState::step_out,
                    ),
                ]
                .into_iter()
                .map(|(id, icon, label, action, step)| {
                    IconButton::new(id, icon)
                        .icon_size(IconSize::Small)
                        .disabled(!is_stopped)
                        .on_click(
                            window.listener_for(running_state, move |state, _, _, cx| {
                                step(state, cx)
                            }),
                        )
                        .tooltip({
                            let focus_handle = focus_handle.clone();
                            move |_, cx| {
                                Tooltip::for_action_in(label, action.as_ref(), &focus_handle, cx)
                            }
                        })
                }),
            )
            .child(Divider::vertical())
            .child(
                IconButton::new("debug-titlebar-restart", IconName::RotateCcw)
                    .icon_size(IconSize::Small)
                    .disabled(state.session().read(cx).is_building())
                    .on_click(window.listener_for(running_state, |state, _, window, cx| {
                        state.rerun_session(window, cx);
                    }))
                    .tooltip({
                        let focus_handle = focus_handle.clone();
                        move |_, cx| {
                            Tooltip::for_action_in(
                                "Rerun Session",
                                &RerunSession,
                                &focus_handle,
                                cx,
                            )
                        }
                    }),
            )
            .child(
                IconButton::new("debug-titlebar-stop", IconName::Stop)
                    .icon_size(IconSize::Small)
                    .icon_color(Color::Error)
                    .on_click(window.listener_for(running_state, |state, _, _, cx| {
                        if state.session().read(cx).is_building()
                            || state.selected_thread_id().is_none()
                        {
                            state.shutdown(cx);
                        } else {
                            state.stop_thread(cx);
                        }
                    }))
                    .tooltip(move |_, cx| {
                        Tooltip::for_action_in("Stop Debugging", &Stop, &focus_handle, cx)
                    }),
            )
            .into_any_element()
    }
}
