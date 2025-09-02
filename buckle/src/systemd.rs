use std::{collections::BTreeMap, time::SystemTime};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use zbus_systemd::{
    systemd1::{ManagerProxy, UnitProxy},
    zbus::connection::Connection,
};

use crate::grpc::{
    GrpcLogDirection, GrpcLogMessage, GrpcUnit, GrpcUnitStatus, UnitEnabledState, UnitLastRunState,
    UnitLoadState, UnitRuntimeState,
};

#[derive(Debug, Clone, Serialize, Eq, PartialEq)]
pub struct LogMessage {
    message: String,
    time: SystemTime,
    service_name: String,
    pid: u64,
    cursor: String,
}

impl From<GrpcLogMessage> for LogMessage {
    fn from(value: GrpcLogMessage) -> Self {
        Self {
            message: value.msg,
            time: SystemTime::UNIX_EPOCH
                + std::time::Duration::from_secs(value.time.unwrap_or_default().seconds as u64),
            service_name: value.service_name,
            pid: value.pid,
            cursor: value.cursor,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Default)]
pub enum LastRunState {
    #[default]
    Failed,
    Dead,
    Mounted,
    Running,
    Listening,
    Plugged,
    Exited,
    Active,
    Waiting,
}

impl std::fmt::Display for LastRunState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            match self {
                Self::Failed => "failed",
                Self::Active => "active",
                Self::Dead => "dead",
                Self::Mounted => "mounted",
                Self::Running => "running",
                Self::Listening => "listening",
                Self::Plugged => "plugged",
                Self::Exited => "exited",
                Self::Waiting => "waiting",
            }
            .into(),
        )
    }
}

impl std::str::FromStr for LastRunState {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        Ok(match s {
            "failed" => Self::Failed,
            "dead" => Self::Dead,
            "mounted" => Self::Mounted,
            "running" => Self::Running,
            "listening" => Self::Listening,
            "plugged" => Self::Plugged,
            "exited" => Self::Exited,
            "active" | "auto-restart" | "auto-restart-queued" => Self::Active,
            "waiting" => Self::Waiting,
            s => return Err(anyhow!("invalid last run state '{}'", s)),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Default)]
pub enum LoadState {
    Loaded,
    #[default]
    Unloaded,
    Inactive,
}

impl std::fmt::Display for LoadState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            match self {
                Self::Loaded => "loaded",
                Self::Unloaded => "not-found",
                Self::Inactive => "inactive",
            }
            .into(),
        )
    }
}

impl std::str::FromStr for LoadState {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        Ok(match s {
            "loaded" | "auto-restart" | "auto-restart-queued" => Self::Loaded,
            "not-found" => Self::Unloaded,
            "inactive" => Self::Inactive,
            s => return Err(anyhow!("invalid load state '{}'", s)),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Default)]
pub enum RuntimeState {
    #[default]
    Started,
    Stopped,
    Restarted,
    Reloaded,
}

impl std::fmt::Display for RuntimeState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            match self {
                Self::Started => "started",
                Self::Stopped => "stopped",
                Self::Restarted => "restarted",
                Self::Reloaded => "reloaded",
            }
            .into(),
        )
    }
}

impl std::str::FromStr for RuntimeState {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        Ok(match s {
            "started" | "running" | "mounted" | "listening" | "plugged" | "active"
            | "activating" => Self::Started,
            "stopped" | "inactive" | "dead" | "failed" | "exited" | "waiting" | "deactivating"
            | "maintenance" => Self::Stopped,
            "restarted" => Self::Restarted,
            "reloaded" => Self::Reloaded,
            s => return Err(anyhow!("invalid runtime state '{}'", s)),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Default)]
pub enum EnabledState {
    #[default]
    Enabled,
    Disabled,
    Failed,
}

impl std::fmt::Display for EnabledState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            match self {
                Self::Enabled => "enabled",
                Self::Disabled => "disabled",
                Self::Failed => "failed",
            }
            .into(),
        )
    }
}

impl std::str::FromStr for EnabledState {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        Ok(match s {
            "enabled" | "active" => Self::Enabled,
            "disabled" | "inactive" => Self::Disabled,
            "failed" => Self::Failed,
            s => return Err(anyhow!("invalid enabled state '{}'", s)),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct UnitSettings {
    pub name: String,
    pub enabled_state: EnabledState,
    pub runtime_state: RuntimeState,
}

impl From<GrpcUnit> for Unit {
    fn from(value: GrpcUnit) -> Self {
        Self {
            name: value.name.clone(),
            description: value.description.clone(),
            enabled_state: value.enabled_state().into(),
            object_path: value.object_path.clone(),
            status: value.status.unwrap_or_default().into(),
        }
    }
}

impl From<Unit> for GrpcUnit {
    fn from(value: Unit) -> Self {
        Self {
            name: value.name.clone(),
            description: value.description.clone(),
            enabled_state: Into::<UnitEnabledState>::into(value.enabled_state).into(),
            object_path: value.object_path.clone(),
            status: Some(value.status.into()),
        }
    }
}

impl From<Status> for GrpcUnitStatus {
    fn from(value: Status) -> Self {
        Self {
            load_state: Into::<UnitLoadState>::into(value.load_state).into(),
            runtime_state: Into::<UnitRuntimeState>::into(value.runtime_state).into(),
            last_run_state: Into::<UnitLastRunState>::into(value.last_run_state).into(),
        }
    }
}

impl From<GrpcUnitStatus> for Status {
    fn from(value: GrpcUnitStatus) -> Self {
        Self {
            load_state: value.load_state().into(),
            runtime_state: value.runtime_state().into(),
            last_run_state: value.last_run_state().into(),
        }
    }
}

impl From<EnabledState> for UnitEnabledState {
    fn from(value: EnabledState) -> Self {
        match value {
            EnabledState::Failed => Self::EnabledFailed,
            EnabledState::Disabled => Self::Disabled,
            EnabledState::Enabled => Self::Enabled,
        }
    }
}

impl From<UnitEnabledState> for EnabledState {
    fn from(value: UnitEnabledState) -> Self {
        match value {
            UnitEnabledState::Enabled => Self::Enabled,
            UnitEnabledState::Disabled => Self::Disabled,
            UnitEnabledState::EnabledFailed => Self::Failed,
        }
    }
}

impl From<LoadState> for UnitLoadState {
    fn from(value: LoadState) -> Self {
        match value {
            LoadState::Loaded => Self::Loaded,
            LoadState::Unloaded => Self::Unloaded,
            LoadState::Inactive => Self::Inactive,
        }
    }
}

impl From<UnitLoadState> for LoadState {
    fn from(value: UnitLoadState) -> Self {
        match value {
            UnitLoadState::Loaded => Self::Loaded,
            UnitLoadState::Unloaded => Self::Unloaded,
            UnitLoadState::Inactive => Self::Inactive,
        }
    }
}

impl From<RuntimeState> for UnitRuntimeState {
    fn from(value: RuntimeState) -> Self {
        match value {
            RuntimeState::Started => Self::Started,
            RuntimeState::Stopped => Self::Stopped,
            RuntimeState::Reloaded => Self::Reloaded,
            RuntimeState::Restarted => Self::Restarted,
        }
    }
}

impl From<UnitRuntimeState> for RuntimeState {
    fn from(value: UnitRuntimeState) -> Self {
        match value {
            UnitRuntimeState::Started => Self::Started,
            UnitRuntimeState::Stopped => Self::Stopped,
            UnitRuntimeState::Reloaded => Self::Reloaded,
            UnitRuntimeState::Restarted => Self::Restarted,
        }
    }
}

impl From<UnitLastRunState> for LastRunState {
    fn from(value: UnitLastRunState) -> Self {
        match value {
            UnitLastRunState::Dead => Self::Dead,
            UnitLastRunState::Exited => Self::Exited,
            UnitLastRunState::Active => Self::Active,
            UnitLastRunState::Mounted => Self::Mounted,
            UnitLastRunState::Running => Self::Running,
            UnitLastRunState::Plugged => Self::Plugged,
            UnitLastRunState::Waiting => Self::Waiting,
            UnitLastRunState::RunFailed => Self::Failed,
            UnitLastRunState::Listening => Self::Listening,
        }
    }
}

impl From<LastRunState> for UnitLastRunState {
    fn from(value: LastRunState) -> Self {
        match value {
            LastRunState::Dead => Self::Dead,
            LastRunState::Exited => Self::Exited,
            LastRunState::Active => Self::Active,
            LastRunState::Mounted => Self::Mounted,
            LastRunState::Running => Self::Running,
            LastRunState::Plugged => Self::Plugged,
            LastRunState::Waiting => Self::Waiting,
            LastRunState::Failed => Self::RunFailed,
            LastRunState::Listening => Self::Listening,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Default)]
pub struct Unit {
    pub name: String,
    pub description: String,
    pub enabled_state: EnabledState,
    pub object_path: String,
    pub status: Status,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Default)]
pub struct Status {
    pub load_state: LoadState,
    pub runtime_state: RuntimeState,
    pub last_run_state: LastRunState,
}

#[derive(Debug, Clone)]
pub struct Systemd {
    client: Connection,
    manager: ManagerProxy<'static>,
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Default, Serialize, Deserialize)]
pub enum LogDirection {
    #[default]
    Forward,
    Backward,
}

impl From<GrpcLogDirection> for LogDirection {
    fn from(value: GrpcLogDirection) -> Self {
        match value {
            GrpcLogDirection::Forward => Self::Forward,
            GrpcLogDirection::Backward => Self::Backward,
        }
    }
}

impl From<LogDirection> for GrpcLogDirection {
    fn from(value: LogDirection) -> Self {
        match value {
            LogDirection::Forward => Self::Forward,
            LogDirection::Backward => Self::Backward,
        }
    }
}

impl Systemd {
    pub async fn new(client: Connection) -> Result<Self> {
        Ok(Self {
            manager: ManagerProxy::new(&client).await?,
            client,
        })
    }

    pub async fn new_session() -> Result<Self> {
        Self::new(Connection::session().await?).await
    }

    pub async fn new_system() -> Result<Self> {
        Self::new(Connection::system().await?).await
    }

    // NOTE: the following functions all take object paths, not systemd service names. To get the
    // object path, use either list() (with a filter) or get_unit().

    pub async fn start(&self, name: String) -> Result<()> {
        self.manager.start_unit(name, "replace".into()).await?;
        Ok(())
    }

    pub async fn stop(&self, name: String) -> Result<()> {
        self.manager.stop_unit(name, "replace".into()).await?;
        Ok(())
    }

    pub async fn restart(&self, name: String) -> Result<()> {
        self.manager.restart_unit(name, "replace".into()).await?;
        Ok(())
    }

    pub async fn reload_unit(&self, name: String) -> Result<()> {
        self.manager.reload_unit(name, "replace".into()).await?;
        Ok(())
    }

    pub async fn reload(&self) -> Result<()> {
        self.manager.reload().await?;
        Ok(())
    }

    pub async fn load_unit(&self, name: String) -> Result<()> {
        self.manager.load_unit(name).await?;
        Ok(())
    }

    pub async fn status(&self, name: String) -> Result<Status> {
        let service = UnitProxy::new(&self.client, name).await?;

        Ok(Status {
            load_state: service.load_state().await?.parse()?,
            runtime_state: service.active_state().await?.parse()?,
            last_run_state: service.sub_state().await?.parse()?,
        })
    }

    // gets the object path for the unit name (f.e., 'sshd.service')
    // required for all the above management calls
    pub async fn get_unit(&self, name: String) -> Result<String> {
        Ok(self.manager.load_unit(name).await?.to_string())
    }

    pub async fn list(&self, filter: Option<String>) -> Result<Vec<Unit>> {
        let list = self.manager.list_units().await?;
        let mut v = Vec::new();
        for item in list {
            let name = item.0;

            if let Some(filter) = &filter {
                if !name.contains(filter) {
                    continue;
                }
            }

            let description = item.1;
            let load_state: LoadState = item.2.parse()?;
            let enabled_state: EnabledState = item.3.parse()?;

            // two kinds of data from one string
            let runtime_state: RuntimeState = item.4.parse()?;
            let last_run_state: LastRunState = item.4.parse()?;

            v.push(Unit {
                name,
                description,
                enabled_state,
                status: Status {
                    load_state,
                    runtime_state,
                    last_run_state,
                },
                // required for all the management calls
                object_path: item.6.to_string(),
            })
        }

        Ok(v)
    }

    pub async fn log(
        &self,
        name: &str,
        count: usize,
        cursor: Option<String>,
        direction: Option<LogDirection>,
    ) -> Result<tokio::sync::mpsc::UnboundedReceiver<BTreeMap<String, String>>> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        let name = name.to_string();
        tokio::spawn(async move {
            let mut journal = systemd::journal::OpenOptions::default()
                .local_only(true)
                .system(true)
                .all_namespaces(true)
                .open()
                .unwrap();

            let journal = journal.match_add("UNIT", name).unwrap();

            // the logic here is:
            // if there is a cursor, seek to it,
            // otherwise, seek to the end and rewind count entries.
            // then, for a direction forward or backward, send count log messages.
            //
            // this leads to weird logic conclusions like "seek to the end, rewind, and then play
            // the previous 100 lines". I think it's better this way because it's consistently
            // weird.

            if cursor.is_some() && !cursor.clone().unwrap().is_empty() {
                journal.seek_cursor(cursor.unwrap()).unwrap();
            } else {
                journal.seek_tail().unwrap();

                // do the seek manually as there is no direct support for seeking by entry count that I
                // can find. this is probably subject to some kind of race condition, but it really
                // doesn't matter unless an extreme amount of log messages arrive in the window between
                // the rewind and fast-forward.
                let mut total = 0;
                while let Ok(Some(_)) = journal.previous_entry() {
                    total += 1;
                    if total > count {
                        break;
                    }
                }
            }

            match direction.unwrap_or_default() {
                LogDirection::Forward => {
                    // FIXME: the struct should really be constructed here, not in the service handler
                    while let Ok(Some(mut entry)) = journal.next_entry() {
                        // Add the cursor so it can be pulled out later
                        entry.insert("CURSOR".into(), journal.cursor().unwrap());
                        tx.send(entry).unwrap()
                    }
                }
                LogDirection::Backward => {
                    // FIXME: the struct should really be constructed here, not in the service handler
                    while let Ok(Some(mut entry)) = journal.previous_entry() {
                        // Add the cursor so it can be pulled out later
                        entry.insert("CURSOR".into(), journal.cursor().unwrap());
                        tx.send(entry).unwrap()
                    }
                }
            }
        });

        Ok(rx)
    }
}

#[cfg(test)]
mod tests {
    use crate::systemd::{LastRunState, RuntimeState, Systemd};

    #[tokio::test]
    async fn test_status() {
        let systemd = Systemd::new_system().await.unwrap();
        let list = systemd.list(None).await.unwrap();
        let mut op = None;
        for item in list {
            // this should be running on any system that tests with zfs
            if item.name == "init.scope" {
                op = Some(item.object_path)
            }
        }
        assert!(op.is_some(), "did not find item in systemd to check");
        let op = op.unwrap();

        assert_eq!(systemd.get_unit("init.scope".into()).await.unwrap(), op);

        let status = systemd.status(op).await.unwrap();
        assert_eq!(status.runtime_state, RuntimeState::Started);
        assert_eq!(status.last_run_state, LastRunState::Running);
    }

    #[tokio::test]
    async fn test_list() {
        let systemd = Systemd::new_system().await.unwrap();
        let list = systemd.list(None).await.unwrap();
        let mut found = false;
        for item in list {
            if item.name == "init.scope" {
                assert_eq!(item.status.last_run_state, LastRunState::Running);
                found = true;
            }
        }
        assert!(found, "did not find item in systemd to check")
    }
}
