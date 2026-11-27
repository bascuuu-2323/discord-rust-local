mod display;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    io::{self, BufRead},
};

#[derive(Deserialize)]
struct Package {
    version: Option<Value>,
    package_sha256: Option<String>,
    package_size: Option<f64>,
}
#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
struct Tasks {
    host_download: Option<Package>,
    module_download: Option<Package>,
    host_install: Option<Package>,
    module_install: Option<Package>,
}
#[derive(Deserialize)]
struct Event {
    task: Tasks,
    state: String,
    percent: Option<f64>,
}
#[derive(Default)]
struct Progress {
    size: Option<f64>,
    percent: f64,
    started: bool,
    complete: bool,
}
impl Progress {
    fn record(&mut self, size: Option<f64>, state: &str, percent: f64) {
        self.size = self.size.or(size);
        self.complete |= state == "Complete";
        self.started |= self.complete || state == "Working";
        self.percent = if self.complete {
            100.
        } else {
            self.percent.max(percent.clamp(0., 99.))
        };
    }
}
#[derive(Default, Serialize)]
struct Phase {
    current: usize,
    total: usize,
    progress: f64,
    started: bool,
    complete: bool,
}
impl Phase {
    fn advance(&mut self, total: usize, done: usize, percent: f64, started: bool, complete: bool) {
        self.total = total;
        self.progress = self.progress.max(percent);
        self.started = started;
        self.complete = complete;
        if started {
            self.current = self.current.max((done + 1).min(total));
        }
    }
}
#[derive(Default, Serialize)]
struct Snapshot {
    downloads: Phase,
    installs: Phase,
}
#[derive(Default)]
struct Pair {
    download: Option<Progress>,
    install: Option<Progress>,
}
impl Pair {
    fn size(&self) -> Option<f64> {
        self.download
            .as_ref()
            .and_then(|p| p.size)
            .or_else(|| self.install.as_ref().and_then(|p| p.size))
    }
}
#[derive(Default)]
struct Tracker {
    packages: BTreeMap<String, Pair>,
    snapshot: Snapshot,
}
impl Tracker {
    fn record(&mut self, e: Event) {
        let t = e.task;
        for (pkg, download) in [
            (t.host_download.or(t.module_download), true),
            (t.host_install.or(t.module_install), false),
        ] {
            if let Some(pkg) = pkg {
                let key = serde_json::to_string(&(pkg.version, pkg.package_sha256))
                    .expect("JSON package identifier");
                let p = self.packages.entry(key).or_default();
                let phase = if download {
                    &mut p.download
                } else {
                    &mut p.install
                };
                phase.get_or_insert_with(Progress::default).record(
                    pkg.package_size,
                    &e.state,
                    e.percent.unwrap_or_default(),
                );
            }
        }
    }
    fn finish(&mut self) {
        for pair in self.packages.values_mut() {
            for phase in [&mut pair.download, &mut pair.install]
                .into_iter()
                .flatten()
            {
                phase.record(None, "Complete", 100.);
            }
        }
    }
    fn snapshot(&mut self) -> &Snapshot {
        let weighted = self
            .packages
            .values()
            .all(|p| p.size().is_some_and(|s| s.is_finite() && s > 0.));
        let (mut weight, mut downloads, mut installs) = (0., 0., 0.);
        let (mut downloaded, mut installed, mut pending) = (0, 0, 0);
        let (mut downloading, mut installing) = (false, false);
        for pair in self.packages.values() {
            let d = pair.download.as_ref();
            let i = pair.install.as_ref();
            let w = if weighted {
                pair.size().unwrap_or(1.)
            } else {
                1.
            };
            let ip = i.map_or(0., |p| p.percent);
            weight += w;
            downloads += w * d.map_or(0., |p| p.percent).max(ip);
            installs += w * ip;
            let received = d.map_or_else(|| i.is_some_and(|p| p.started), |p| p.complete);
            downloaded += usize::from(received);
            installed += usize::from(i.map_or(received, |p| p.complete));
            pending += usize::from(d.is_some_and(|p| !p.complete));
            downloading |= received || d.is_some_and(|p| p.started);
            installing |= i.is_some_and(|p| p.started);
        }
        let total = self.packages.len();
        let percent = |sum| if weight > 0. { sum / weight } else { 0. };
        self.snapshot.downloads.advance(
            total,
            downloaded,
            percent(downloads),
            downloading,
            total > 0 && pending == 0,
        );
        self.snapshot.installs.advance(
            total,
            installed,
            percent(installs),
            installing,
            total > 0 && installed == total,
        );
        &self.snapshot
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let display = std::env::args().nth(1).as_deref() == Some("--display");
    let mut tracker = Tracker::default();
    for line in io::stdin().lock().lines() {
        let line = line?;
        if display {
            if !line.trim().is_empty() {
                let state: display::State = serde_json::from_str(&line)?;
                println!("{}", serde_json::to_string(&state.display())?);
            }
            continue;
        }
        match line.trim() {
            "" => continue,
            "finish" => tracker.finish(),
            "snapshot" => (),
            event => tracker.record(serde_json::from_str(event)?),
        }
        println!("{}", serde_json::to_string(tracker.snapshot())?);
    }
    Ok(())
}
