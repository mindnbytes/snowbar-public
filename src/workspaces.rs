//! Pure state for the real niri workspace slice.
//!
//! This module has no Rune or socket dependencies. It reduces niri events into a
//! view model, decides the single-output display policy, and validates focus
//! intents. Allside effects (sockets, threads, repaint wakes) live in `niri.rs`
//! and `main.rs`.

use niri_ipc::{Action, Event, Workspace, WorkspaceReferenceArg};

/// Rune's startup view of one display, reduced to what the output policy needs.
/// A display without a Wayland registry name cannot be targeted and is ignored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisplaySnapshot {
    pub wayland_id: Option<u32>,
}

/// The one validated single-output binding. It is only valid for `generation`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputBinding {
    /// niri's name for its sole output, associated by cardinality (not by name equality).
    pub niri_output: String,
    /// Rune's startup `wl_output` registry name for its sole usable Wayland display.
    pub wayland_display: u32,
    pub generation: u64,
}

/// Result of the deliberately strict single-output selection policy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DisplaySelection {
    /// Exactly one targetable Rune display and exactly one niri output.
    Bound(OutputBinding),
    /// Zero, multiple, or untargetable candidates. Fail closed: no focus actions.
    Ambiguous,
}

/// Decide the output binding from Rune's startup snapshot and niri's output
/// inventory. Both sides must have unambiguous cardinality; names are *not*
/// compared because the diagnostic showed no reliable relationship between
/// Rune's registry ID and niri's output name.
pub fn select_display(
    displays: &[DisplaySnapshot],
    niri_outputs: &[String],
    generation: u64,
) -> DisplaySelection {
    let mut ids = displays.iter().filter_map(|display| display.wayland_id);
    let (Some(wayland_display), None) = (ids.next(), ids.next()) else {
        return DisplaySelection::Ambiguous;
    };
    let [niri_output] = niri_outputs else {
        return DisplaySelection::Ambiguous;
    };
    DisplaySelection::Bound(OutputBinding {
        niri_output: niri_output.clone(),
        wayland_display,
        generation,
    })
}

/// One workspace as the view needs it: stable ID for actions, current index for
/// the numeral label, and flags for the approved treatments.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkspaceItem {
    pub id: u64,
    pub index: u8,
    pub is_active: bool,
    pub is_urgent: bool,
}

/// A focus request that is valid only for the session that produced it.
#[derive(Clone, Debug)]
pub struct FocusIntent {
    pub generation: u64,
    pub workspace_id: u64,
    pub action: Action,
}

/// Messages sent from the IPC effect thread to the UI. They are consumed in
/// order, so a newer generation always supersedes an older one.
#[derive(Clone, Debug)]
pub enum IpcMessage {
    /// A new connection attempt started; no snapshot is usable yet.
    Session { generation: u64 },
    /// A fresh snapshot arrived on a validated output mapping.
    Ready {
        generation: u64,
        output: String,
        workspaces: Vec<Workspace>,
    },
    /// A dynamic change on an established stream.
    Event { generation: u64, event: Event },
}

/// Current workspace state, owned by the UI and advanced by ordered messages.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkspaceState {
    generation: u64,
    output: Option<String>,
    connected: bool,
    /// A snapshot mentioned an output other than the bound one: fail closed.
    drift: bool,
    workspaces: Vec<Workspace>,
}

impl WorkspaceState {
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn output(&self) -> Option<&str> {
        self.output.as_deref()
    }

    /// True only with a fresh snapshot on a mapping that has not drifted.
    pub fn is_available(&self) -> bool {
        self.connected && self.output.is_some() && !self.drift
    }

    /// Apply an ordered IPC message. This is the only entry point the UI uses.
    pub fn apply(&mut self, message: IpcMessage) {
        match message {
            IpcMessage::Session { generation } => {
                self.generation = generation;
                self.output = None;
                self.connected = false;
                self.drift = false;
                self.workspaces.clear();
            }
            IpcMessage::Ready {
                generation,
                output,
                workspaces,
            } => {
                self.generation = generation;
                self.output = Some(output);
                self.workspaces = workspaces;
                self.connected = true;
                self.drift = self.detect_drift();
            }
            IpcMessage::Event { generation, event } => {
                if self.generation != generation {
                    return;
                }
                self.apply_event(&event);
            }
        }
    }

    /// Apply a dynamic niri event. Events other than a replacement snapshot are
    /// ignored until a fresh `Ready` snapshot arrives, so a reconnect cannot
    /// revive stale or partial state.
    pub fn apply_event(&mut self, event: &Event) {
        match event {
            Event::WorkspacesChanged { workspaces } => {
                self.workspaces.clone_from(workspaces);
                self.drift = self.detect_drift();
            }
            _ if !self.connected => {}
            Event::WorkspaceUrgencyChanged { id, urgent } => {
                if let Some(workspace) = self.workspaces.iter_mut().find(|ws| ws.id == *id) {
                    workspace.is_urgent = *urgent;
                }
            }
            Event::WorkspaceActivated { id, focused } => {
                let Some(activated) = self.workspaces.iter().find(|ws| ws.id == *id) else {
                    return;
                };
                let output = activated.output.clone();
                for workspace in &mut self.workspaces {
                    if workspace.output == output {
                        workspace.is_active = workspace.id == *id;
                    }
                    if *focused {
                        workspace.is_focused = workspace.id == *id;
                    }
                }
            }
            Event::WorkspaceActiveWindowChanged {
                workspace_id,
                active_window_id,
            } => {
                if let Some(workspace) =
                    self.workspaces.iter_mut().find(|ws| ws.id == *workspace_id)
                {
                    workspace.active_window_id = *active_window_id;
                }
            }
            _ => {}
        }
    }

    /// A non-empty, different output name in a global snapshot means the output
    /// inventory changed; this is our best available change signal without an
    /// niri output event or a Rune display callback.
    fn detect_drift(&self) -> bool {
        let Some(expected) = self.output.as_deref() else {
            return true;
        };
        self.workspaces.iter().any(|ws| {
            ws.output
                .as_deref()
                .is_some_and(|output| output != expected)
        })
    }

    /// Every workspace on `output`, including the trailing empty workspace,
    /// ordered by current per-output index. Names are never used as labels.
    pub fn for_output<'a>(&'a self, output: &str) -> Vec<&'a Workspace> {
        if !self.connected || self.drift {
            return Vec::new();
        }
        let mut workspaces: Vec<_> = self
            .workspaces
            .iter()
            .filter(|workspace| workspace.output.as_deref() == Some(output))
            .collect();
        workspaces.sort_by_key(|workspace| (workspace.idx, workspace.id));
        workspaces
    }

    /// The view model, empty whenever the mapping is not trustworthy.
    pub fn view(&self) -> Vec<WorkspaceItem> {
        let Some(output) = self.output.as_deref() else {
            return Vec::new();
        };
        self.for_output(output)
            .into_iter()
            .map(|ws| WorkspaceItem {
                id: ws.id,
                index: ws.idx,
                is_active: ws.is_active,
                is_urgent: ws.is_urgent,
            })
            .collect()
    }

    /// Look up a current workspace by its stable niri ID.
    pub fn by_id(&self, id: u64) -> Option<&Workspace> {
        (self.connected && !self.drift)
            .then(|| self.workspaces.iter().find(|workspace| workspace.id == id))
            .flatten()
    }

    /// Create a focus intent only for a workspace in the current snapshot on a
    /// trustworthy mapping. The effects layer must still re-check the generation
    /// immediately before sending, because a disconnect can race with a click.
    pub fn focus_action(&self, id: u64) -> Option<FocusIntent> {
        self.by_id(id).map(|_| FocusIntent {
            generation: self.generation,
            workspace_id: id,
            action: Action::FocusWorkspace {
                reference: WorkspaceReferenceArg::Id(id),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace(
        id: u64,
        idx: u8,
        name: Option<&str>,
        output: Option<&str>,
        is_active: bool,
    ) -> Workspace {
        Workspace {
            id,
            idx,
            name: name.map(str::to_owned),
            output: output.map(str::to_owned),
            is_urgent: false,
            is_active,
            is_focused: is_active,
            active_window_id: None,
        }
    }

    fn ready(state: &mut WorkspaceState, workspaces: Vec<Workspace>) {
        state.apply(IpcMessage::Ready {
            generation: state.generation(),
            output: "Virtual-1".to_owned(),
            workspaces,
        });
    }

    #[test]
    fn display_policy_requires_one_wayland_display_and_one_niri_output() {
        let one = [DisplaySnapshot {
            wayland_id: Some(44),
        }];
        let two = [
            DisplaySnapshot {
                wayland_id: Some(44),
            },
            DisplaySnapshot {
                wayland_id: Some(45),
            },
        ];
        let unnamed = [DisplaySnapshot { wayland_id: None }];
        assert_eq!(
            select_display(&one, &["Virtual-1".to_owned()], 7),
            DisplaySelection::Bound(OutputBinding {
                niri_output: "Virtual-1".to_owned(),
                wayland_display: 44,
                generation: 7,
            })
        );
        assert_eq!(
            select_display(&[], &["Virtual-1".to_owned()], 7),
            DisplaySelection::Ambiguous
        );
        assert_eq!(
            select_display(&unnamed, &["Virtual-1".to_owned()], 7),
            DisplaySelection::Ambiguous
        );
        assert_eq!(
            select_display(&two, &["Virtual-1".to_owned()], 7),
            DisplaySelection::Ambiguous
        );
        assert_eq!(select_display(&one, &[], 7), DisplaySelection::Ambiguous);
        assert_eq!(
            select_display(&one, &["A".to_owned(), "B".to_owned()], 7),
            DisplaySelection::Ambiguous
        );
    }

    #[test]
    fn view_shows_all_indices_in_order_including_trailing_and_multi_digit() {
        let mut state = WorkspaceState::default();
        ready(
            &mut state,
            vec![
                workspace(10, 10, Some("ten"), Some("Virtual-1"), false),
                workspace(2, 2, Some("named"), Some("Virtual-1"), true),
                workspace(1, 1, None, Some("Virtual-1"), false),
                workspace(3, 3, None, Some("Virtual-1"), false),
            ],
        );
        assert!(state.is_available());
        let view = state.view();
        assert_eq!(
            view.iter().map(|item| item.index).collect::<Vec<_>>(),
            [1, 2, 3, 10]
        );
        assert_eq!(
            view.iter().map(|item| item.id).collect::<Vec<_>>(),
            [1, 2, 3, 10]
        );
        assert!(view[1].is_active);
    }

    #[test]
    fn dynamic_events_update_urgency_activation_and_active_window() {
        let mut state = WorkspaceState::default();
        ready(
            &mut state,
            vec![
                workspace(1, 1, None, Some("Virtual-1"), true),
                workspace(2, 2, None, Some("Virtual-1"), false),
                workspace(3, 3, None, Some("Virtual-1"), false),
            ],
        );
        state.apply(IpcMessage::Event {
            generation: state.generation(),
            event: Event::WorkspaceUrgencyChanged {
                id: 2,
                urgent: true,
            },
        });
        state.apply(IpcMessage::Event {
            generation: state.generation(),
            event: Event::WorkspaceActivated {
                id: 2,
                focused: true,
            },
        });
        state.apply(IpcMessage::Event {
            generation: state.generation(),
            event: Event::WorkspaceActiveWindowChanged {
                workspace_id: 2,
                active_window_id: Some(44),
            },
        });
        assert!(!state.by_id(1).unwrap().is_active);
        assert!(state.by_id(2).unwrap().is_active);
        assert!(state.by_id(2).unwrap().is_focused);
        assert!(!state.by_id(3).unwrap().is_active);
        assert!(!state.by_id(3).unwrap().is_focused);
        assert!(state.by_id(2).unwrap().is_urgent);
        assert_eq!(state.by_id(2).unwrap().active_window_id, Some(44));
    }

    #[test]
    fn snapshot_replaces_removed_workspaces_and_updates_indices() {
        let mut state = WorkspaceState::default();
        ready(
            &mut state,
            vec![
                workspace(7, 3, None, Some("Virtual-1"), true),
                workspace(8, 4, None, Some("Virtual-1"), false),
            ],
        );
        state.apply(IpcMessage::Event {
            generation: state.generation(),
            event: Event::WorkspacesChanged {
                workspaces: vec![workspace(7, 1, Some("renamed"), Some("Virtual-1"), true)],
            },
        });
        let view = state.view();
        assert_eq!(view.len(), 1);
        assert_eq!(view[0].id, 7);
        assert_eq!(view[0].index, 1);
        assert!(state.by_id(8).is_none());
    }

    #[test]
    fn session_before_snapshot_and_after_disconnect_forbids_actions() {
        let mut state = WorkspaceState::default();
        state.apply(IpcMessage::Session { generation: 1 });
        assert!(!state.is_available());
        assert!(state.view().is_empty());
        assert!(state.focus_action(1).is_none());
        state.apply(IpcMessage::Event {
            generation: 1,
            event: Event::WorkspaceUrgencyChanged {
                id: 1,
                urgent: true,
            },
        });
        assert!(state.view().is_empty());

        ready(
            &mut state,
            vec![workspace(1, 1, None, Some("Virtual-1"), true)],
        );
        assert!(state.focus_action(1).is_some());

        // A reconnect starts a new generation, dropping all stale IDs at once.
        state.apply(IpcMessage::Session { generation: 2 });
        assert!(!state.is_available());
        assert!(state.focus_action(1).is_none());
        state.apply(IpcMessage::Event {
            generation: 1,
            event: Event::WorkspacesChanged {
                workspaces: vec![workspace(1, 1, None, Some("Virtual-1"), true)],
            },
        });
        assert!(!state.is_available(), "old-generation events are ignored");
        assert!(state.focus_action(1).is_none());
    }

    #[test]
    fn output_drift_fails_closed_until_a_fresh_generation() {
        let mut state = WorkspaceState::default();
        ready(
            &mut state,
            vec![workspace(1, 1, None, Some("Virtual-1"), true)],
        );
        state.apply(IpcMessage::Event {
            generation: state.generation(),
            event: Event::WorkspacesChanged {
                workspaces: vec![
                    workspace(1, 1, None, Some("Virtual-1"), true),
                    workspace(2, 1, None, Some("Virtual-2"), true),
                ],
            },
        });
        assert!(!state.is_available());
        assert!(state.view().is_empty());
        assert!(state.focus_action(1).is_none());

        state.apply(IpcMessage::Session { generation: 9 });
        state.apply(IpcMessage::Ready {
            generation: 9,
            output: "Virtual-1".to_owned(),
            workspaces: vec![workspace(1, 1, None, Some("Virtual-1"), true)],
        });
        assert!(state.is_available());
        assert!(state.focus_action(1).is_some());
    }

    #[test]
    fn repeated_activation_without_a_change_keeps_state_equal() {
        let mut state = WorkspaceState::default();
        ready(
            &mut state,
            vec![
                workspace(1, 1, None, Some("Virtual-1"), true),
                workspace(2, 2, None, Some("Virtual-1"), false),
            ],
        );
        state.apply(IpcMessage::Event {
            generation: state.generation(),
            event: Event::WorkspaceActivated {
                id: 2,
                focused: true,
            },
        });
        let before = state.clone();
        // niri can repeat the current activation; `poll` only asks for a repaint
        // when the reduced state actually differs.
        state.apply(IpcMessage::Event {
            generation: state.generation(),
            event: Event::WorkspaceActivated {
                id: 2,
                focused: true,
            },
        });
        assert_eq!(state, before);

        state.apply(IpcMessage::Event {
            generation: state.generation(),
            event: Event::WorkspaceActivated {
                id: 1,
                focused: true,
            },
        });
        assert_ne!(state, before);
        assert!(state.by_id(1).unwrap().is_active);
        assert!(!state.by_id(2).unwrap().is_active);
    }

    #[test]
    fn focus_intent_uses_current_stable_id_and_generation() {
        let mut state = WorkspaceState::default();
        assert!(state.focus_action(42).is_none());
        ready(
            &mut state,
            vec![workspace(42, 2, Some("named"), Some("Virtual-1"), true)],
        );
        let intent = state.focus_action(42).unwrap();
        assert_eq!(intent.generation, state.generation());
        assert_eq!(intent.workspace_id, 42);
        assert!(matches!(
            intent.action,
            Action::FocusWorkspace {
                reference: WorkspaceReferenceArg::Id(42)
            }
        ));
        assert!(state.focus_action(2).is_none(), "index is not the ID");
    }
}
