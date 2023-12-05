mod recording_watcher;

use std::collections::HashSet;
use tracing::info;
use crate::recording_watcher::{DebugActor, RecordingWatcher};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().init();
    info!("Startup..");

    let mut devices_in_scope = HashSet::new();
    devices_in_scope.insert("Built-in Audio Analog Stereo".to_string());

    let mut devices_ignored = HashSet::new();
    devices_ignored.insert("PulseAudio Volume Control".to_string());

    RecordingWatcher::new(devices_in_scope, devices_ignored, DebugActor{}).start_watcher()
}
