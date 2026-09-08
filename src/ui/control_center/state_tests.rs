use super::state::{
    ConfirmationState, ConfirmationTarget, ControlCenterRuntimeSnapshot, DisplayRollbackStatus,
    DisplaySession, InteractionState,
};
use crate::keyboard::binding::ModifierMask;
use crate::ui::layout::ElementId;
use crate::ui::presentation::DisplayWizardStep;

#[test]
fn display_session_keeps_edit_step_dirty_state_and_selection_coherent() {
    let mut session = DisplaySession::default();
    assert!(!session.is_editing());
    assert!(!session.is_dirty());
    assert_eq!(session.step(), None);
    assert_eq!(session.selected_route(), None);

    session.open(DisplayWizardStep::Displays, false);
    session.select_first_route(2);
    assert!(session.is_editing());
    assert!(!session.is_dirty());
    assert_eq!(session.step(), Some(DisplayWizardStep::Displays));
    assert_eq!(session.selected_route(), Some(0));

    session.mark_dirty();
    session.set_step(DisplayWizardStep::Review);
    assert!(session.is_dirty());
    assert_eq!(session.step(), Some(DisplayWizardStep::Review));

    session.close();
    assert!(!session.is_editing());
    assert!(!session.is_dirty());
    assert_eq!(session.step(), None);
    assert_eq!(session.selected_route(), None);
}

#[test]
fn display_session_never_invents_route_zero_for_an_empty_profile() {
    let mut session = DisplaySession::default();
    session.open(DisplayWizardStep::Displays, false);
    session.select_first_route(0);
    assert_eq!(session.selected_route(), None);

    session.select_route(Some(3));
    session.clamp_selected_route(0);
    assert_eq!(session.selected_route(), None);
}

#[test]
fn hotkey_capture_owns_target_and_modifiers_as_one_state() {
    let mut interaction = InteractionState::default();
    assert!(!interaction.capture_active());
    assert_eq!(interaction.capture_target(), None);
    assert_eq!(interaction.capture_modifiers(), ModifierMask::NONE);

    interaction.start_capture(ElementId::CycleOutputHotkey);
    interaction.set_capture_modifiers(ModifierMask::CTRL.union(ModifierMask::ALT));
    assert!(interaction.capture_active());
    assert_eq!(
        interaction.capture_target(),
        Some(ElementId::CycleOutputHotkey)
    );
    assert_eq!(
        interaction.capture_modifiers(),
        ModifierMask::CTRL.union(ModifierMask::ALT)
    );

    assert!(interaction.clear_capture());
    assert!(!interaction.capture_active());
    assert_eq!(interaction.capture_target(), None);
    assert_eq!(interaction.capture_modifiers(), ModifierMask::NONE);
    assert!(!interaction.clear_capture());
}

#[test]
fn confirmation_state_allows_only_one_pending_destructive_action() {
    let mut confirmations = ConfirmationState::default();
    assert!(!confirmations.request_or_consume(ConfirmationTarget::ResetSettings));
    assert!(confirmations.is_pending(ConfirmationTarget::ResetSettings));

    assert!(!confirmations.request_or_consume(ConfirmationTarget::DeleteDisplayProfile));
    assert!(!confirmations.is_pending(ConfirmationTarget::ResetSettings));
    assert!(confirmations.is_pending(ConfirmationTarget::DeleteDisplayProfile));

    assert!(confirmations.request_or_consume(ConfirmationTarget::DeleteDisplayProfile));
    assert!(!confirmations.is_pending(ConfirmationTarget::DeleteDisplayProfile));
}

#[test]
fn rollback_snapshot_projects_transport_flags_into_one_semantic_state() {
    let mut snapshot = ControlCenterRuntimeSnapshot::default();
    assert_eq!(
        snapshot.display_rollback_status(),
        DisplayRollbackStatus::Inactive
    );

    snapshot.set_display_rollback_phase(true, true);
    assert_eq!(
        snapshot.display_rollback_status(),
        DisplayRollbackStatus::Testing { error: None }
    );

    snapshot.display_rollback_error = Some("test failed".into());
    assert_eq!(
        snapshot.display_rollback_status(),
        DisplayRollbackStatus::Testing {
            error: Some("test failed".into())
        }
    );

    snapshot.display_rollback_error = None;
    snapshot.set_display_rollback_phase(true, false);
    assert_eq!(
        snapshot.display_rollback_status(),
        DisplayRollbackStatus::Recovering
    );

    snapshot.set_display_rollback_phase(false, true);
    assert!(!snapshot.display_keep_available);
    assert_eq!(
        snapshot.display_rollback_status(),
        DisplayRollbackStatus::Inactive
    );
}
