use pipewire::prelude::ReadableDict;
use pipewire::spa::{ForeignDict, ParsableValue};
use pipewire::types::ObjectType;
use pipewire::{Context, MainLoop};
use snafu::prelude::*;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use tracing::{info, debug, warn};

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("No output node id present in properties"))]
    NoOutputNode { props: String },
    #[snafu(display("No input node id present in properties"))]
    NoInputNode { props: String },
}

#[derive(Default)]
pub struct State {
    headset_id: Option<u32>,
    active_links: HashSet<u32>,
    on_air: bool,
}

impl State {
    pub fn is_link_in_scope(&self, output_node: &u32) -> bool {
        self.headset_id.map_or(false, |id| id.eq(output_node))
    }

    fn update_on_air(&mut self) {
        let current_on_air = self.check_if_on_air();
        if current_on_air != self.on_air {
            // states don't match, update
            info!("On Air state changed from [{}] to [{}], running hook..", self.on_air, current_on_air);
            self.on_air = current_on_air;
            self.run_on_air_hook();
        }
    }

    pub fn set_headset_id(&mut self, id :&u32) {
        self.headset_id = Some(id.clone());
        self.update_on_air();
    }

    pub fn add_link(&mut self, id: &u32) {
        self.active_links.insert(id.clone());
        self.update_on_air()
    }

    pub fn remove_link(&mut self, id: &u32) {
        self.active_links.remove(id);
        self.update_on_air();
    }

   fn run_on_air_hook(&self) -> Result<(), Error> {
       Ok(())
   }

    pub fn check_if_on_air(&self) -> bool {
        self.on_air
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().init();
    let local_registry: Arc<RwLock<HashMap<u32, String>>> = Arc::new(RwLock::new(HashMap::new()));
    let state: Arc<RwLock<State>> = Arc::new(RwLock::new(State::default()));
    let global_state = state.clone();
    let remove_state = state.clone();

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
                        if let Some(node_description) = node_props.get("node.description") {
                            if node_description.eq("Jabra Engage 75 Mono") {
                                info!("Identified id [{}] as headset", global.id);
                                global_state.clone().write().unwrap().set_headset_id(&global.id);
                            }
                            local_registry
                                .clone()
                                .write()
                                .unwrap()
                                .insert(global.id, node_description.to_string());
                        };
                    }
                    debug!("done with node [{}]", global.id);
                }

                ObjectType::Link => {
                    let reg = local_registry.clone();
                    if let Some(link_props) = &global.props {
                        let local_state = global_state.clone();
                        let state_read = local_state.read().unwrap();
                        let input_node =
                            u32::parse_value(get_input_node(&link_props).unwrap()).unwrap();
                        let output_node =
                            u32::parse_value(get_output_node(&link_props).unwrap()).unwrap();
                        if state_read.is_link_in_scope(&output_node) {
                            // Need to drop read here, otherwise no writy below
                            drop(state_read);
                            info!("found in scope link [{}] from [{}] to [{}]", global.id, output_node, input_node);
                            global_state
                                .clone()
                                .write()
                                .unwrap()
                                .add_link(&global.id);
                            info!("On Air: [{:?}]", global_state.clone().read().unwrap().check_if_on_air());
                            debug!("dropped write lock for updating id [{}]", global.id);
                        } else {
                            let reg_read = reg.read().unwrap();
                            debug!(
                                "New Link: [{:?}] from [{}] to [{}]",
                                global.id,
                                reg_read
                                    .get(&output_node)
                                    .unwrap_or(&"undefined".to_string()),
                                reg_read
                                    .get(&input_node)
                                    .unwrap_or(&&"undefined".to_string())
                            );
                        }
                    }
                    debug!("done with link [{}]", global.id);
                }
                _ => {
                    // Other objects are not interesting to us
                }
            };
        })
        .global_remove(move |id| {
            if remove_state.clone().read().unwrap().active_links.contains(&id) {
                info!("In scope link [{}] removed.", id);
                remove_state.clone().write().unwrap().remove_link(&id);
                info!("On Air: [{:?}]", remove_state.clone().read().unwrap().check_if_on_air());

            }
        })
        .register();
    mainloop.run();
    Ok(())
}

pub fn get_input_node(props: &ForeignDict) -> Result<&str, Error> {
    props.get("link.input.node").context(NoInputNodeSnafu {
        props: format!("{:?}", props),
    })
}

pub fn get_output_node(props: &ForeignDict) -> Result<&str, Error> {
    props.get("link.output.node").context(NoOutputNodeSnafu {
        props: format!("{:?}", props),
    })
}


