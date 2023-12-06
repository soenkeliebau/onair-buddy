use pipewire::prelude::ReadableDict;
use pipewire::spa::{ForeignDict, ParsableValue};
use pipewire::types::ObjectType;
use pipewire::{Context, MainLoop, keys};
use snafu::prelude::*;
use std::collections::{HashMap, HashSet};
use std::process::Command;
use std::string::ToString;
use std::sync::{Arc, RwLock};
use tracing::{debug, info, warn};

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("No output node id present in properties"))]
    NoOutputNode { props: String },
    #[snafu(display("No input node id present in properties"))]
    NoInputNode { props: String },
}

pub trait OnAirActor {
    fn go_on_air(&self);
    fn go_off_air(&self);
}

pub struct DebugActor {}

impl OnAirActor for DebugActor {
    fn go_on_air(&self) {
        warn!("going on air!");
        Command::new("sh")
            .arg("-c")
            .arg("notify-send \"Going on air!\"")
            .output()
            .expect("failed to execute process");
    }

    fn go_off_air(&self) {
        warn!("going off air!");
        Command::new("sh")
            .arg("-c")
            .arg("notify-send \"Going off air!\"")
            .output()
            .expect("failed to execute process");
    }
}
pub struct RecordingWatcher<T>
where
    T: OnAirActor,
{
    state: Arc<RwLock<State<T>>>,
}

impl<T: OnAirActor + 'static> RecordingWatcher<T> {
    pub fn new(
        devices_in_scope: HashSet<String>,
        devices_ignored: HashSet<String>,
        actor: T,
    ) -> Self {
        RecordingWatcher {
            state: Arc::new(RwLock::new(State::new(
                devices_in_scope,
                devices_ignored,
                actor,
            ))),
        }
    }

    pub fn start_watcher(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        //let local_registry: Arc<RwLock<HashMap<u32, String>>> = Arc::new(RwLock::new(HashMap::new()));
        //let state: Arc<RwLock<State>> = Arc::new(RwLock::new(State::default()));
        let global_state = self.state.clone();
        let remove_state = self.state.clone();

        let mainloop = MainLoop::new()?;
        let context = Context::new(&mainloop)?;
        let core = context.connect(None)?;
        let registry = core.get_registry()?;

        let _listener = registry
            .add_listener_local()
            .global(move |global| {
                match global.type_ {
                    ObjectType::Node => {
                        if let Some(node_props) = &global.props {
                            global_state
                                .clone()
                                .write()
                                .unwrap()
                                .add_node(global.id, node_props);
                        };
                        debug!("done with node [{}]", global.id);
                    }

                    ObjectType::Link => {
                        if let Some(link_props) = &global.props {
                            global_state
                                .clone()
                                .write()
                                .unwrap()
                                .add_link(&global.id, link_props);
                        }
                        debug!("done with link [{}]", global.id);
                    }
                    _ => {

                        //println!("[{}]-{:?}", global.id, global.props);
                    }
                };
            })
            .global_remove(move |id| {
                if remove_state
                    .clone()
                    .read()
                    .unwrap()
                    .active_links
                    .contains(&id)
                {
                    info!("In scope link [{}] removed.", id);
                    remove_state.clone().write().unwrap().remove_link(&id);
                    info!(
                        "On Air: [{:?}]",
                        remove_state.clone().read().unwrap().check_if_on_air()
                    );
                }
            })
            .register();
        mainloop.run();
        Ok(())
    }
}

struct State<T> where T: OnAirActor {
    devices_in_scope: HashSet<String>,
    devices_ignored: HashSet<String>,
    ids_in_scope: HashSet<u32>,
    ids_ignored: HashSet<u32>,
    active_links: HashSet<u32>,
    on_air: bool,
    registry: HashMap<u32, String>,
    actor: T,
}

impl<T> State<T> where T:OnAirActor{
    pub fn new(
        devices_in_scope: HashSet<String>,
        devices_ignored: HashSet<String>,
        actor: T,
    ) -> Self {
        let mut registry: HashMap<u32, String> = HashMap::new();
        registry.insert(u32::MAX, "unresolved".to_string());
        State {
            devices_in_scope,
            devices_ignored,
            ids_in_scope: HashSet::new(),
            ids_ignored: HashSet::new(),
            active_links: HashSet::new(),
            on_air: false,
            registry,
            actor,
        }
    }
    pub fn is_link_in_scope(&self, output_node: &u32) -> bool {
        self.ids_in_scope.contains(output_node)
    }

    fn update_on_air(&mut self) {
        let current_state = self.on_air;
        let target_state = !self.active_links.is_empty();
        if current_state != target_state {
            // states don't match, update
            info!(
                "On Air state changed from [{}] to [{}], running hook..",
                current_state, target_state
            );
            self.on_air = target_state;
            if target_state {
                info!("running on air hook");
                self.run_on_air_hook();
            } else {
                info!("running off air hook");
                self.run_off_air_hook();
            }
        }
    }

    pub fn add_headset_id(&mut self, id: &u32) {
        self.ids_in_scope.insert(id.clone());
        self.update_on_air();
    }

    pub fn add_link(&mut self, id: &u32, props: &ForeignDict) {
        let input_node = u32::parse_value(get_input_node(props).unwrap()).unwrap();
        let output_node = u32::parse_value(get_output_node(props).unwrap()).unwrap();
        if self.ids_in_scope.contains(&output_node) {
            if !self.ids_ignored.contains(&input_node) {
                info!(
                    "found in scope link [{}] from [{}] to [{}]",
                    id, output_node, input_node
                );
                info!("id:[{}] - {:?}", id, props);
                self.active_links.insert(id.clone());
            } else {
                info!(
                    "Ignoring link [{}] from [{}] to [{}] due to node [{}] being in ignore list",
                    id, output_node, input_node, input_node
                );
            }
        }
        self.update_on_air()
    }

    pub fn add_node(&mut self, id: u32, props: &ForeignDict) {
        let node_names = get_all_names(props);
        if !node_names.is_empty() { //let Some(node_name) = props.get("node.description") {
            let primary_name = node_names.first().unwrap();
            debug!("Processing node [{:?}]", primary_name);
            self.registry.insert(id, primary_name.to_string());

            // Check if any name is in both lists
            if node_names.iter().map(|name| self.devices_in_scope.contains(&name.to_string())).any(|present| present) {
                info!(
                    "Adding id [{}] as in scope due to matching node name [{}]",
                    id, primary_name
                );
                self.ids_in_scope.insert(id);
            }

            if node_names.iter().map(|name| self.devices_ignored.contains(&name.to_string())).any(|present| present) {
                info!(
                    "Adding id [{}] as ignored due to matching node name [{}]",
                    id, primary_name
                );
                self.ids_ignored.insert(id);
            }
        }
    }

    pub fn resolve_node_id(&self, id: &u32) -> &str {
        self.registry
            .get(id)
            .unwrap_or_else(|| self.registry.get(&u32::MAX).unwrap())
    }

    pub fn remove_link(&mut self, id: &u32) {
        self.active_links.remove(id);
        self.update_on_air();
    }

    fn run_on_air_hook(&self) -> Result<(), Error> {
        self.actor.go_on_air();
        Ok(())
    }

    fn run_off_air_hook(&self) -> Result<(), Error> {
        self.actor.go_off_air();
        Ok(())
    }

    pub fn check_if_on_air(&self) -> bool {
        self.on_air
    }
}

fn get_input_node(props: &ForeignDict) -> Result<&str, Error> {
    props.get("link.input.node").context(NoInputNodeSnafu {
        props: format!("{:?}", props),
    })
}

fn get_output_node(props: &ForeignDict) -> Result<&str, Error> {
    props.get("link.output.node").context(NoOutputNodeSnafu {
        props: format!("{:?}", props),
    })
}

fn get_all_names(props: &ForeignDict) -> Vec<&str> {
    [&keys::NODE_DESCRIPTION, &keys::NODE_NICK, &keys::NODE_NAME]
        .into_iter()
        .map(|prop_name| props.get(prop_name))
        .flatten()
        .collect()
}
