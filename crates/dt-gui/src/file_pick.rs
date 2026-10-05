//! "Choose files": the system's own open dialog, run through a tool every desktop already
//! has (PowerShell on Windows, AppleScript on a Mac, zenity or kdialog on Linux), so no
//! dialog crate ships in the binary. The dialog runs on a thread; the page polls it.

use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::{Receiver, TryRecvError, channel};

/// A dialog that is open.
pub struct Picking {
    rx: Receiver<Result<Vec<PathBuf>, String>>,
}

impl Picking {
    /// Opens the dialog for PNG, SVG and zip files, several at once.
    pub fn open(ctx: &eframe::egui::Context) -> Picking {
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(run());
            ctx.request_repaint();
        });
        Picking { rx }
    }

    /// The chosen files once the dialog closes (empty when cancelled), or why it could not
    /// open.
    pub fn poll(&self) -> Option<Result<Vec<PathBuf>, String>> {
        match self.rx.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Ok(Vec::new())),
        }
    }
}

const NO_DIALOG: &str = "Couldn't open a file window here. Drag your files onto DeadTune instead.";

fn lines(out: &[u8]) -> Vec<PathBuf> {
    String::from_utf8_lossy(out)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(PathBuf::from)
        .collect()
}

#[cfg(windows)]
fn run() -> Result<Vec<PathBuf>, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let script = "[Console]::OutputEncoding = [Text.Encoding]::UTF8; \
        Add-Type -AssemblyName System.Windows.Forms; \
        $owner = New-Object System.Windows.Forms.Form -Property @{TopMost = $true}; \
        $d = New-Object System.Windows.Forms.OpenFileDialog; \
        $d.Title = 'Choose images or a collection'; \
        $d.Filter = 'Images and collections (*.png;*.svg;*.zip)|*.png;*.svg;*.zip'; \
        $d.Multiselect = $true; \
        if ($d.ShowDialog($owner) -eq 'OK') { $d.FileNames -join \"`n\" }";
    let out = Command::new("powershell")
        .args(["-NoProfile", "-STA", "-Command", script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|_| NO_DIALOG.to_string())?;
    Ok(lines(&out.stdout))
}

#[cfg(target_os = "macos")]
fn run() -> Result<Vec<PathBuf>, String> {
    let out = Command::new("osascript")
        .args([
            "-e",
            "set picked to choose file with prompt \"Choose images or a collection\" of type {\"png\", \"svg\", \"zip\"} with multiple selections allowed",
            "-e",
            "set out to \"\"",
            "-e",
            "repeat with f in picked",
            "-e",
            "set out to out & POSIX path of f & linefeed",
            "-e",
            "end repeat",
            "-e",
            "return out",
        ])
        .output()
        .map_err(|_| NO_DIALOG.to_string())?;
    Ok(lines(&out.stdout))
}

#[cfg(not(any(windows, target_os = "macos")))]
fn run() -> Result<Vec<PathBuf>, String> {
    let zenity = Command::new("zenity")
        .args([
            "--file-selection",
            "--multiple",
            "--separator=\n",
            "--title=Choose images or a collection",
            "--file-filter=Images and collections | *.png *.svg *.zip",
        ])
        .output();
    if let Ok(out) = zenity {
        return Ok(lines(&out.stdout));
    }
    let out = Command::new("kdialog")
        .args([
            "--getopenfilename",
            ".",
            "*.png *.svg *.zip",
            "--multiple",
            "--separate-output",
        ])
        .output()
        .map_err(|_| NO_DIALOG.to_string())?;
    Ok(lines(&out.stdout))
}
