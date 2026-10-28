// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Running a command script in a headless run (#340): each command when its
//! step has come, through the same handlers as a key (`DomainCommandHandler`)
//! or a sidebar tool (the operator commands).

use crate::application::operator_command::OperatorCommand;
use crate::application::operator_command::source::{
    AdjustSourceSphereEmitPeriod, AdjustSourceSphereSpeed, CycleSourceSphereSpecies,
    PlaceSourceSphere, ToggleSourceSphere,
};
use crate::application::operator_command::thermostat::{
    AdjustThermostatRadius, AdjustThermostatTargetTemperature, PlaceThermostat, ToggleThermostat,
};
use crate::application::{
    CommandHandler, DomainCommandHandler, PhysicsEvent, SnapshotPublisher, domain_command_spec,
};
use crate::export::SampleStream;
use crate::headless_script::{
    CommandRecord, OperatorAction, ScheduledItem, Script, ScriptAction, ScriptItem,
};
use crate::metrics::{MetricsRegistry, SharedMetricsRegistry};
use std::io;
use std::sync::mpsc;
use thermo_physics::application::MainContent;
use thermo_physics::objects2d::Vec2D;

/// The commands of a script still to run, and what the domain command
/// handler needs besides the world.
pub(super) struct ScriptRun {
    schedule: Vec<ScheduledItem>,
    next: usize,
    publisher: SnapshotPublisher,
    events: (mpsc::Sender<PhysicsEvent>, mpsc::Receiver<PhysicsEvent>),
    /// Never started by a script (`world.toggle_sample_stream_*` is not
    /// scriptable); the handler needs the slot.
    stream: Option<SampleStream>,
}

impl ScriptRun {
    pub(super) fn new(script: &Script) -> Self {
        Self {
            schedule: script.schedule(),
            next: 0,
            publisher: SnapshotPublisher::new(),
            events: mpsc::channel(),
            stream: None,
        }
    }

    /// Run every command due once `steps_done` steps have run; returns their
    /// records. `metrics` is what `world.export_series_*` exports.
    pub(super) fn run_due(
        &mut self,
        steps_done: u32,
        application: &mut MainContent,
        metrics: Option<&SharedMetricsRegistry>,
    ) -> io::Result<Vec<CommandRecord>> {
        let mut records = Vec::new();
        while let Some(scheduled) = self.schedule.get(self.next) {
            if scheduled.at_step > steps_done {
                break;
            }
            let item = scheduled.item.clone();
            self.next += 1;
            let step = application.world().step_count() as u64;
            self.run_item(&item, application, metrics)
                .map_err(|message| {
                    io::Error::other(format!("script line {}: {message}", item.line))
                })?;
            records.push(CommandRecord::of(&item, step));
        }
        Ok(records)
    }

    fn run_item(
        &mut self,
        item: &ScriptItem,
        application: &mut MainContent,
        metrics: Option<&SharedMetricsRegistry>,
    ) -> Result<(), String> {
        match &item.action {
            ScriptAction::Step(_) => Ok(()),
            ScriptAction::Domain { id, arg } => {
                let spec =
                    domain_command_spec(id).ok_or_else(|| format!("unknown command `{id}`"))?;
                let fallback;
                let metrics = match metrics {
                    Some(metrics) => metrics,
                    None => {
                        fallback = MetricsRegistry::shared();
                        &fallback
                    }
                };
                let mut handler = DomainCommandHandler::new(
                    application,
                    &mut self.publisher,
                    &self.events.0,
                    metrics,
                    &mut self.stream,
                );
                handler.handle_command(spec.build(*arg));
                handler.flush();
                self.report_events()
            }
            ScriptAction::Operator { action, .. } => run_operator(*action, application),
        }
    }

    /// Files written by `world.save` / `world.export_*` are named on stderr
    /// (their names hold the time, so they stay out of the records); a
    /// failure fails the run.
    fn report_events(&self) -> Result<(), String> {
        while let Ok(event) = self.events.1.try_recv() {
            match event {
                PhysicsEvent::WorldSaved(path) => eprintln!("saved world: {}", path.display()),
                PhysicsEvent::DataExported { path, note } => {
                    eprintln!("exported: {}", path.display());
                    if let Some(note) = note {
                        eprintln!("{note}");
                    }
                }
                PhysicsEvent::SaveFailed(error) => return Err(format!("saving failed: {error}")),
                PhysicsEvent::ExportFailed(error) => return Err(format!("export failed: {error}")),
                _ => {}
            }
        }
        Ok(())
    }
}

/// The id of the `index`-th thermostat or source sphere (creation order).
fn operator_id(application: &MainContent, index: usize, thermostat: bool) -> Result<usize, String> {
    // From the world, not a snapshot: a snapshot copies every ball.
    let world = application.world();
    let (ids, kind): (Vec<usize>, &str) = if thermostat {
        let thermostats = world.thermostats().thermostats();
        (thermostats.iter().map(|t| t.id).collect(), "thermostat")
    } else {
        let spheres = world.source_spheres().spheres();
        (spheres.iter().map(|s| s.id).collect(), "source sphere")
    };
    ids.get(index)
        .copied()
        .ok_or_else(|| format!("there is no {kind} {index} ({} placed)", ids.len()))
}

fn run_operator(action: OperatorAction, application: &mut MainContent) -> Result<(), String> {
    use OperatorAction as A;
    let thermostat = |app: &MainContent, index| operator_id(app, index, true);
    let sphere = |app: &MainContent, index| operator_id(app, index, false);
    let command: Box<dyn OperatorCommand> = match action {
        A::PlaceThermostat { x, y } => Box::new(PlaceThermostat {
            position: Vec2D::new(x, y),
        }),
        A::ToggleThermostat { index } => Box::new(ToggleThermostat {
            thermostat_id: thermostat(application, index)?,
        }),
        A::AdjustThermostatTemperature { index, steps } => {
            Box::new(AdjustThermostatTargetTemperature {
                thermostat_id: thermostat(application, index)?,
                delta_steps: steps,
            })
        }
        A::AdjustThermostatRadius { index, steps } => Box::new(AdjustThermostatRadius {
            thermostat_id: thermostat(application, index)?,
            delta_steps: steps,
        }),
        A::PlaceSourceSphere { x, y } => Box::new(PlaceSourceSphere {
            center: Vec2D::new(x, y),
        }),
        A::ToggleSourceSphere { index } => Box::new(ToggleSourceSphere {
            sphere_id: sphere(application, index)?,
        }),
        A::AdjustSourceSphereSpeed { index, delta } => Box::new(AdjustSourceSphereSpeed {
            sphere_id: sphere(application, index)?,
            delta,
        }),
        A::AdjustSourceSpherePeriod { index, delta_steps } => {
            Box::new(AdjustSourceSphereEmitPeriod {
                sphere_id: sphere(application, index)?,
                delta_steps,
            })
        }
        A::CycleSourceSphereSpecies { index } => Box::new(CycleSourceSphereSpecies {
            sphere_id: sphere(application, index)?,
        }),
    };
    command.execute(application);
    Ok(())
}
