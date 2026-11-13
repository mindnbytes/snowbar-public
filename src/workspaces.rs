//! Reducer for niri workspace event-stream state.

use niri_ipc::{Action, Event, Workspace, WorkspaceReferenceArg};

/// Current workspace state from one niri event stream.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkspaceState {
    connected: bool,
    workspaces: Vec<Workspace>,
}

impl WorkspaceState {
    /// Whether a fresh workspace snapshot has been received on this connection.
    pub fn is_available(&self) -> bool {
        self.connected
    }

    /// Drop stale IDs immediately. The next `WorkspacesChanged` event starts the new session.
    pub fn disconnect(&mut self) {
        self.connected = false;
        self.workspaces.clear();
    }

    /// Apply an official niri event. Events other than a replacement snapshot are ignored until
    /// that snapshot arrives, so reconnects cannot revive stale or partial state.
    pub fn apply(&mut self, event: &Event) {
        match event {
            Event::WorkspacesChanged { workspaces } => {
                self.workspaces.clone_from(workspaces);
                self.connected = true;
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

    /// Every workspace on `output`, including the trailing empty workspace, ordered by current
    /// per-output index. Names are deliberately not used as labels or filters.
    pub fn for_output<'a>(&'a self, output: &str) -> Vec<&'a Workspace> {
        if !self.connected {
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

    /// Look up a current workspace by its stable niri ID (for actions, not display ordering).
    pub fn by_id(&self, id: u64) -> Option<&Workspace> {
        self.connected
            .then(|| self.workspaces.iter().find(|workspace| workspace.id == id))
            .flatten()
    }

    /// Create a focus intent only for a workspace in the current snapshot.
    /// The effects layer must still guard queued intents across disconnects.
    pub fn focus_action(&self, id: u64) -> Option<Action> {
        self.by_id(id).map(|_| Action::FocusWorkspace {
            reference: WorkspaceReferenceArg::Id(id),
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

    fn snapshot(state: &mut WorkspaceState, workspaces: Vec<Workspace>) {
        state.apply(&Event::WorkspacesChanged { workspaces });
    }

    #[test]
    fn returns_all_workspaces_in_output_order_including_trailing_and_named() {
        let mut state = WorkspaceState::default();
        snapshot(
            &mut state,
            vec![
                workspace(9, 9, None, Some("A"), false),
                workspace(2, 2, Some("named"), Some("A"), true),
                workspace(20, 1, None, Some("B"), true),
                workspace(1, 1, None, Some("A"), false),
                workspace(3, 3, None, Some("A"), false),
                workspace(4, 4, None, Some("A"), false),
                workspace(5, 5, None, Some("A"), false),
                workspace(6, 6, None, Some("A"), false),
                workspace(7, 7, None, Some("A"), false),
                workspace(8, 8, None, Some("A"), false),
                workspace(10, 10, None, Some("A"), false),
            ],
        );
        let a = state.for_output("A");
        assert_eq!(
            a.iter().map(|ws| ws.id).collect::<Vec<_>>(),
            [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
        );
        assert_eq!(a[1].idx, 2); // Numeric label source, not the workspace name.
        assert_eq!(
            state
                .for_output("B")
                .iter()
                .map(|ws| ws.id)
                .collect::<Vec<_>>(),
            [20]
        );
        assert!(state.for_output("missing").is_empty());
    }

    #[test]
    fn events_update_urgency_activation_and_active_window() {
        let mut state = WorkspaceState::default();
        snapshot(
            &mut state,
            vec![
                workspace(1, 1, None, Some("A"), true),
                workspace(2, 2, None, Some("A"), false),
                workspace(3, 1, None, Some("B"), true),
            ],
        );
        state.apply(&Event::WorkspaceUrgencyChanged {
            id: 2,
            urgent: true,
        });
        state.apply(&Event::WorkspaceActivated {
            id: 2,
            focused: true,
        });
        state.apply(&Event::WorkspaceActiveWindowChanged {
            workspace_id: 2,
            active_window_id: Some(44),
        });
        assert!(!state.by_id(1).unwrap().is_active);
        assert!(state.by_id(2).unwrap().is_active);
        assert!(state.by_id(2).unwrap().is_focused);
        assert!(state.by_id(3).unwrap().is_active);
        assert!(!state.by_id(3).unwrap().is_focused);
        assert!(state.by_id(2).unwrap().is_urgent);
        assert_eq!(state.by_id(2).unwrap().active_window_id, Some(44));
    }

    #[test]
    fn snapshot_replaces_removed_workspaces_and_disconnect_requires_fresh_snapshot() {
        let mut state = WorkspaceState::default();
        snapshot(&mut state, vec![workspace(1, 1, None, Some("A"), true)]);
        assert!(state.by_id(1).is_some());
        state.disconnect();
        assert!(!state.is_available());
        assert!(state.by_id(1).is_none());
        state.apply(&Event::WorkspaceUrgencyChanged {
            id: 1,
            urgent: true,
        });
        assert!(state.for_output("A").is_empty());

        snapshot(&mut state, vec![workspace(2, 1, None, Some("A"), true)]);
        assert!(state.by_id(1).is_none());
        assert!(state.by_id(2).is_some());
    }

    #[test]
    fn updates_indices_and_output_membership_without_changing_ids() {
        let mut state = WorkspaceState::default();
        snapshot(&mut state, vec![workspace(7, 3, None, Some("A"), true)]);
        snapshot(
            &mut state,
            vec![workspace(7, 1, Some("renamed"), Some("B"), true)],
        );
        assert_eq!(state.for_output("A").len(), 0);
        let current = state.for_output("B");
        assert_eq!(current[0].id, 7);
        assert_eq!(current[0].idx, 1);
    }

    #[test]
    fn unfocused_activation_and_unknown_ids_do_not_change_other_outputs() {
        let mut state = WorkspaceState::default();
        let mut focused = workspace(1, 1, None, Some("A"), true);
        focused.is_focused = true;
        let mut other = workspace(3, 1, None, Some("B"), true);
        other.is_focused = false;
        snapshot(
            &mut state,
            vec![focused, other, workspace(4, 2, None, Some("B"), false)],
        );
        state.apply(&Event::WorkspaceActivated {
            id: 4,
            focused: false,
        });
        state.apply(&Event::WorkspaceActivated {
            id: 99,
            focused: true,
        });
        assert!(state.by_id(1).unwrap().is_focused);
        assert!(state.by_id(1).unwrap().is_active);
        assert!(!state.by_id(3).unwrap().is_active);
        assert!(state.by_id(4).unwrap().is_active);
        assert!(!state.by_id(4).unwrap().is_focused);
    }

    #[test]
    fn focus_intent_uses_current_stable_id_and_rejects_stale_ids() {
        let mut state = WorkspaceState::default();
        assert!(state.focus_action(42).is_none());
        snapshot(
            &mut state,
            vec![workspace(42, 2, Some("named"), Some("A"), true)],
        );
        assert!(matches!(
            state.focus_action(42),
            Some(Action::FocusWorkspace {
                reference: WorkspaceReferenceArg::Id(42)
            })
        ));
        assert!(state.focus_action(2).is_none()); // Index is not the ID.
        state.disconnect();
        assert!(state.focus_action(42).is_none());
        snapshot(&mut state, vec![workspace(43, 2, None, Some("A"), true)]);
        assert!(state.focus_action(42).is_none());
    }
}
