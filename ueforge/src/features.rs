//! Feature builder for mod init.
//!
//! Every UE mod has features that apply at different times: once
//! at init, on each game load, when a DataTable streams in. This
//! module collects them in one place with consistent logging and
//! a single `.install()` call.

use std::time::Duration;

type Finder = Box<dyn Fn() -> Option<*const u8> + Send + 'static>;
type OnLoad = Box<dyn Fn(*const u8) + Send + 'static>;
type OnTable = Box<dyn FnOnce(&'static crate::ue::UObject) + Send + 'static>;
type OnceAction = Box<dyn FnOnce() + Send + 'static>;

enum Trigger {
    Once {
        label: &'static str,
        action: OnceAction,
    },
    EachLoad {
        label: &'static str,
        poll: Duration,
        finder: Finder,
        on_load: OnLoad,
    },
    FirstTable {
        label: &'static str,
        table: &'static str,
        timeout: Duration,
        on_ready: OnTable,
    },
}

pub struct Features {
    triggers: Vec<Trigger>,
}

pub fn features() -> Features {
    Features { triggers: Vec::new() }
}

impl Features {
    pub fn once<F>(mut self, label: &'static str, action: F) -> Self
    where
        F: FnOnce() + Send + 'static,
    {
        self.triggers.push(Trigger::Once {
            label,
            action: Box::new(action),
        });
        self
    }

    pub fn on_each_load<P, A>(
        mut self,
        label: &'static str,
        poll: Duration,
        finder: P,
        on_load: A,
    ) -> Self
    where
        P: Fn() -> Option<*const u8> + Send + 'static,
        A: Fn(*const u8) + Send + 'static,
    {
        self.triggers.push(Trigger::EachLoad {
            label,
            poll,
            finder: Box::new(finder),
            on_load: Box::new(on_load),
        });
        self
    }

    pub fn on_first_table<F>(
        mut self,
        label: &'static str,
        table: &'static str,
        timeout: Duration,
        on_ready: F,
    ) -> Self
    where
        F: FnOnce(&'static crate::ue::UObject) + Send + 'static,
    {
        self.triggers.push(Trigger::FirstTable {
            label,
            table,
            timeout,
            on_ready: Box::new(on_ready),
        });
        self
    }

    pub fn install(self) {
        let count = self.triggers.len();
        crate::log::log(format_args!("features: installing {count}"));

        for trigger in self.triggers {
            match trigger {
                Trigger::Once { label, action } => {
                    crate::log::log(format_args!("feature {label}: applying"));
                    action();
                    crate::log::log(format_args!("feature {label}: done"));
                }
                Trigger::EachLoad { label, poll, finder, on_load } => {
                    crate::log::log(format_args!("feature {label}: watching"));
                    crate::ue::actor::on_each_load(label, poll, finder, on_load);
                }
                Trigger::FirstTable { label, table, timeout, on_ready } => {
                    crate::log::log(format_args!(
                        "feature {label}: waiting for table {table}"
                    ));
                    crate::ue::datatable::on_first_sight(table, timeout, move |dt| {
                        crate::log::log(format_args!("feature {label}: table ready"));
                        on_ready(dt);
                        crate::log::log(format_args!("feature {label}: done"));
                    });
                }
            }
        }

        crate::log::log(format_args!("features: all {count} installed"));
    }
}
