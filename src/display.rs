use serde::{Deserialize, Serialize};

#[derive(Default, Deserialize)]
struct Phase {
    current: Option<u64>,
    #[serde(default)]
    total: u64,
    #[serde(default)]
    progress: f64,
    #[serde(default)]
    complete: bool,
}
#[derive(Deserialize)]
pub struct State {
    #[serde(default)]
    status: String,
    #[serde(flatten)]
    fallback: Phase,
    seconds: Option<u64>,
    downloads: Option<Phase>,
    installs: Option<Phase>,
}

#[derive(Serialize)]
struct Fills {
    back: f64,
    front: f64,
}
#[derive(Serialize)]
pub struct Display {
    text: String,
    fills: Option<Fills>,
}

impl State {
    pub fn display(&self) -> Display {
        let mut fills = None;
        let text = match self.status.as_str() {
            "launching" => "Starting…".into(),
            "update-failure" => format!(
                "Update failed — retrying in {} sec…",
                self.seconds.unwrap_or(0)
            ),
            "downloading-updates" | "installing-updates" => {
                let (downloading, phase) = match (&self.downloads, &self.installs) {
                    (Some(d), Some(i)) => {
                        if d.total > 0 || i.total > 0 {
                            fills = Some(Fills {
                                back: d.progress,
                                front: i.progress.min(d.progress),
                            });
                        }
                        if d.total > 0 && !d.complete {
                            (true, d)
                        } else {
                            (false, i)
                        }
                    }
                    _ => (self.status == "downloading-updates", &self.fallback),
                };
                let verb = if downloading {
                    "Downloading"
                } else {
                    "Installing"
                };
                if phase.current == Some(0) {
                    format!("{verb} updates…")
                } else {
                    format!(
                        "{verb} update {} of {}…",
                        phase.current.unwrap_or(0),
                        phase.total
                    )
                }
            }
            _ => "Checking for updates…".into(),
        };
        Display { text, fills }
    }
}
