/**
 * UnsavedChangesDialog: confirmation dialog shown when the user attempts
 * to navigate away (tab switch, item selection, user switch) while an
 * editor has unsaved changes. Offers Save, Discard, and Cancel actions.
 *
 * Wraps the existing ModalDialog component and reuses the application's
 * standard button classes.
 */

import { Component } from "solid-js";
import ModalDialog from "./ModalDialog";

interface UnsavedChangesDialogProps {
  /** Whether the dialog is currently visible. */
  open: boolean;
  /** Save current changes, then proceed with navigation. */
  onSave: () => void;
  /** Discard changes and proceed with navigation. */
  onDiscard: () => void;
  /** Abort navigation, keep current state. */
  onCancel: () => void;
}

const UnsavedChangesDialog: Component<UnsavedChangesDialogProps> = (props) => {
  return (
    <ModalDialog
      title="Unsaved Changes"
      open={props.open}
      onClose={props.onCancel}
    >
      <p style={{ margin: "0 0 1.5rem 0", color: "var(--text-secondary)" }}>
        You have unsaved changes that will be lost.
      </p>
      <div style={{ display: "flex", gap: "0.5rem", "justify-content": "flex-end" }}>
        <button class="btn" onClick={props.onCancel}>
          Cancel
        </button>
        <button class="btn btn-danger" onClick={props.onDiscard}>
          Discard
        </button>
        <button class="btn btn-primary" onClick={props.onSave}>
          Save
        </button>
      </div>
    </ModalDialog>
  );
};

export default UnsavedChangesDialog;
