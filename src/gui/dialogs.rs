use eframe::egui;
use std::path::PathBuf;

/// User's choice when prompted about unsaved changes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsavedChangesChoice {
    /// Save changes before proceeding
    Save,
    /// Discard changes and proceed
    DontSave,
    /// Cancel the operation
    Cancel,
}

/// Action that was deferred pending unsaved changes confirmation
#[derive(Debug, Clone)]
pub enum PendingAction {
    /// User clicked "New"
    NewProject,
    /// User clicked "Open" and selected a file
    OpenConfig(PathBuf),
    /// User is trying to close the window
    CloseWindow,
}

/// Dialog shown when user has unsaved changes
pub struct UnsavedChangesDialog {
    pub pending_action: PendingAction,
}

impl UnsavedChangesDialog {
    pub fn new(pending_action: PendingAction) -> Self {
        Self { pending_action }
    }

    /// Show the dialog, returns Some(choice) when user makes a selection
    pub fn show(&mut self, ctx: &egui::Context) -> Option<UnsavedChangesChoice> {
        let mut result = None;

        egui::Window::new("Unsaved Changes")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label("You have unsaved changes. What would you like to do?");
                ui.add_space(12.0);

                ui.horizontal(|ui| {
                    if ui.button("Don't Save").clicked() {
                        result = Some(UnsavedChangesChoice::DontSave);
                    }
                    if ui.button("Cancel").clicked() {
                        result = Some(UnsavedChangesChoice::Cancel);
                    }
                    if ui.button("Save").clicked() {
                        result = Some(UnsavedChangesChoice::Save);
                    }
                });
            });

        result
    }
}

/// State for the config file chooser dialog
pub struct ConfigChooserDialog {
    pub bento_files: Vec<PathBuf>,
    pub selected_index: usize,
}

impl ConfigChooserDialog {
    pub fn new(bento_files: Vec<PathBuf>) -> Self {
        Self {
            bento_files,
            selected_index: 0,
        }
    }

    /// Returns Some(path) if user selected a file, None if still choosing
    pub fn show(&mut self, ctx: &egui::Context) -> Option<PathBuf> {
        let mut result = None;

        egui::Window::new("Choose Config File")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label("Multiple .bento files found. Select one:");
                ui.add_space(8.0);

                for (i, path) in self.bento_files.iter().enumerate() {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| path.display().to_string());

                    if ui
                        .selectable_label(self.selected_index == i, &name)
                        .clicked()
                    {
                        self.selected_index = i;
                    }
                }

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Open").clicked() {
                        result = Some(self.bento_files[self.selected_index].clone());
                    }
                    if ui.button("Cancel").clicked() {
                        result = Some(PathBuf::new()); // Empty path = cancelled
                    }
                });
            });

        result
    }
}

/// Find all .bento files in a directory
pub fn find_bento_files(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|e| e == "bento") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}
