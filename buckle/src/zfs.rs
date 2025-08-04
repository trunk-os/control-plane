use crate::grpc::{
    ZfsDataset, ZfsEntry, ZfsList, ZfsModifyDataset, ZfsModifyVolume, ZfsType, ZfsVolume,
};
use anyhow::{anyhow, Result};
use fancy_duration::AsFancyDuration;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, str::FromStr};
use tracing::{debug, error, trace};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ZFSKind {
    Dataset,
    Volume,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Dataset {
    pub name: String,
    pub quota: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ModifyDataset {
    pub name: String,
    pub modifications: Dataset,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Volume {
    pub name: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ModifyVolume {
    pub name: String,
    pub modifications: Volume,
}

#[derive(Debug, Clone)]
pub struct Pool {
    name: String,
    controller: Controller,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZFSStat {
    pub kind: ZFSKind,
    pub name: String,
    pub full_name: String,
    pub size: u64,
    pub used: u64,
    pub avail: u64,
    pub refer: u64,
    pub mountpoint: Option<String>,
    // FIXME collect options (like quotas)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZFSOutputInfo {
    command: String,
    vers_major: u64,
    vers_minor: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZFSGet<T> {
    output_version: ZFSOutputInfo,
    datasets: HashMap<String, ZFSGetItem<T>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZFSGetItem<T> {
    name: String,
    #[serde(rename = "type")]
    typ: String,
    pool: String,
    createtxg: u64,
    properties: HashMap<String, ZFSValue<T>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZFSList {
    output_version: ZFSOutputInfo,
    datasets: HashMap<String, ZFSListItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZFSListItem {
    name: String,
    #[serde(rename = "type")]
    typ: String,
    pool: String,
    createtxg: u64,
    properties: ZFSListItemProperties,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZFSListItemProperties {
    used: ZFSValue<u64>,
    available: ZFSValue<u64>,
    referenced: ZFSValue<u64>,
    mountpoint: ZFSValue<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZFSValue<T> {
    value: T,
    source: ZFSSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZFSSource {
    #[serde(rename = "type")]
    typ: String,
    data: String,
}

impl From<ModifyVolume> for ZfsModifyVolume {
    fn from(value: ModifyVolume) -> Self {
        Self {
            name: value.name,
            modifications: Some(value.modifications.into()),
        }
    }
}

impl From<ZfsModifyVolume> for ModifyVolume {
    fn from(value: ZfsModifyVolume) -> Self {
        Self {
            name: value.name,
            modifications: value.modifications.unwrap_or_default().into(),
        }
    }
}

impl From<ModifyDataset> for ZfsModifyDataset {
    fn from(value: ModifyDataset) -> Self {
        Self {
            name: value.name,
            modifications: Some(value.modifications.into()),
        }
    }
}

impl From<ZfsModifyDataset> for ModifyDataset {
    fn from(value: ZfsModifyDataset) -> Self {
        Self {
            name: value.name,
            modifications: value.modifications.unwrap_or_default().into(),
        }
    }
}

impl From<Dataset> for ZfsDataset {
    fn from(value: Dataset) -> Self {
        Self {
            name: value.name,
            quota: value.quota,
        }
    }
}

impl From<ZfsDataset> for Dataset {
    fn from(value: ZfsDataset) -> Self {
        Self {
            name: value.name,
            quota: value.quota,
        }
    }
}

impl From<Volume> for ZfsVolume {
    fn from(value: Volume) -> Self {
        Self {
            name: value.name,
            size: value.size,
        }
    }
}

impl From<ZfsVolume> for Volume {
    fn from(value: ZfsVolume) -> Self {
        Self {
            name: value.name,
            size: value.size,
        }
    }
}

impl From<ZfsList> for Vec<ZFSStat> {
    fn from(value: ZfsList) -> Self {
        let mut list = Self::default();
        for item in value.entries {
            list.push(item.into())
        }
        list
    }
}

impl From<Vec<ZFSStat>> for ZfsList {
    fn from(value: Vec<ZFSStat>) -> Self {
        let mut list = Self::default();
        for item in value {
            list.entries.push(item.into())
        }
        list
    }
}

impl From<ZfsEntry> for ZFSStat {
    fn from(value: ZfsEntry) -> Self {
        Self {
            kind: match value.kind() {
                ZfsType::Volume => ZFSKind::Volume,
                ZfsType::Dataset => ZFSKind::Dataset,
            },
            name: value.name,
            full_name: value.full_name,
            size: value.size,
            used: value.used,
            avail: value.avail,
            refer: value.refer,
            mountpoint: value.mountpoint,
        }
    }
}

impl From<ZFSStat> for ZfsEntry {
    fn from(value: ZFSStat) -> Self {
        Self {
            kind: match value.kind {
                ZFSKind::Volume => ZfsType::Volume,
                ZFSKind::Dataset => ZfsType::Dataset,
            }
            .into(),
            name: value.name,
            full_name: value.full_name,
            size: value.size,
            used: value.used,
            avail: value.avail,
            refer: value.refer,
            mountpoint: value.mountpoint,
        }
    }
}

impl Pool {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            controller: Controller,
        }
    }

    pub fn create_dataset(&self, info: &Dataset) -> Result<()> {
        let mut options: Option<CommandOptions> = None;

        if let Some(quota) = &info.quota {
            let mut tmp = CommandOptions::default();
            tmp.insert("quota".to_string(), format!("{}", quota));
            options = Some(tmp);
        }

        if let Err(e) = self
            .controller
            .create_dataset(&self.name, &info.name, options)
        {
            error!("Creating dataset: {}", e.to_string());
            return Err(e);
        }

        self.controller.mount(&self.name)?;

        Ok(())
    }

    pub fn create_volume(&self, info: &Volume) -> Result<()> {
        if let Err(e) = self
            .controller
            .create_volume(&self.name, &info.name, info.size, None)
        {
            error!("Creating volume: {}", e.to_string());
            return Err(e);
        }
        Ok(())
    }

    pub fn modify_dataset(&self, info: ModifyDataset) -> Result<()> {
        let mut map = HashMap::default();
        if let Some(quota) = &info.modifications.quota {
            map.insert("quota", format!("{}", quota));
        }

        if let Err(e) = self.controller.set(&self.name, &info.name, map) {
            error!("Setting options on dataset: {}", e.to_string());
            return Err(e);
        }

        if info.modifications.name != "" && info.name != info.modifications.name {
            self.controller.unmount(&self.name, &info.name)?;

            if let Err(e) = self
                .controller
                .rename(&self.name, &info.name, &info.modifications.name)
            {
                error!("Renaming dataset: {}", e.to_string());
                return Err(e);
            }

            self.controller.mount(&self.name)?;
        }

        Ok(())
    }

    pub fn modify_volume(&self, info: ModifyVolume) -> Result<()> {
        let mut map = HashMap::default();
        if info.modifications.size != 0 {
            map.insert("volsize", format!("{}", info.modifications.size));
        }

        if let Err(e) = self.controller.set(&self.name, &info.name, map) {
            error!("Setting options on volume: {}", e.to_string());
            return Err(e);
        }

        if info.modifications.name != "" && info.name != info.modifications.name {
            if let Err(e) = self
                .controller
                .rename(&self.name, &info.name, &info.modifications.name)
            {
                error!("Renaming volume: {}", e.to_string());
                return Err(e);
            }
        }

        Ok(())
    }

    pub fn destroy(&self, name: String) -> Result<()> {
        if let Err(e) = self.controller.destroy(&self.name, &name) {
            error!("Destroying dataset: {}", e.to_string());
            return Err(e);
        }

        Ok(())
    }

    pub fn list(&self, filter: Option<String>) -> Result<Vec<ZFSStat>> {
        let mut ret = Vec::new();
        let list = match self.controller.list() {
            Ok(x) => x,
            Err(e) => {
                error!("Listing datasets: {}", e.to_string());
                return Err(e);
            }
        };

        for (name, item) in list.datasets {
            if let Some(filter) = &filter {
                if !item.name.starts_with(&format!("{}/{}", self.name, filter)) {
                    continue;
                }
            }

            if !name.starts_with(&self.name) {
                continue;
            }

            if name == self.name {
                // skip root-level datasets since they correspond to pools
                continue;
            }

            let short_name = name
                .strip_prefix(&format!("{}/", self.name))
                .unwrap_or_else(|| &name)
                .to_owned();

            ret.push(ZFSStat {
                // volumes don't have a mountpath, '-' is indicated
                // FIXME relying on datasets being mounted is a thing we're doing right now, it'll
                //       probably have to change eventually, but zfs handles all the mounting for
                //       us at create and destroy time.
                kind: if item.typ == "VOLUME" {
                    ZFSKind::Volume
                } else {
                    ZFSKind::Dataset
                },
                full_name: name.clone(),
                name: short_name.clone(), // strip the pool
                used: item.properties.used.value,
                avail: item.properties.available.value,
                // this is just easier to use in places
                size: if item.typ == "VOLUME" {
                    match self.controller.get(&self.name, &short_name, "volsize") {
                        Ok(x) => x,
                        Err(e) => {
                            error!("Getting volume size for {}: {}", name, e.to_string());
                            return Err(e);
                        }
                    }
                } else {
                    let quota = self
                        .controller
                        .get(&self.name, &short_name, "quota")
                        .unwrap_or_default();

                    if quota != 0 {
                        quota
                    } else {
                        self.controller.get(&self.name, &short_name, "available")?
                    }
                },
                refer: item.properties.referenced.value,
                mountpoint: if item.properties.mountpoint.value == "-" {
                    None
                } else {
                    Some(item.properties.mountpoint.value)
                },
            })
        }
        Ok(ret)
    }
}

#[derive(Debug, Clone, Default)]
struct CommandOptions(HashMap<String, String>);

impl CommandOptions {
    fn to_options(&self) -> Vec<String> {
        let mut args = Vec::new();
        for (key, value) in &self.0 {
            args.push("-o".to_string());
            args.push(format!("{}={}", key, value));
        }
        args
    }
}

impl std::ops::Deref for CommandOptions {
    type Target = HashMap<String, String>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for CommandOptions {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Debug, Clone, Default)]
struct Controller;

impl Controller {
    fn run(command: &str, args: Vec<String>) -> Result<String> {
        debug!("Running command: [{}, {}]", command, args.join(", "));
        let time = std::time::Instant::now();

        let out = match std::process::Command::new(command)
            .args(args.clone())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
        {
            Ok(x) => x,
            Err(e) => {
                error!(
                    "Error running command: [{}, {}]: {}",
                    command,
                    args.join(", "),
                    e.to_string()
                );
                return Err(e.into());
            }
        };

        trace!(
            "ZFS command took {}",
            (std::time::Instant::now() - time).fancy_duration()
        );

        if out.status.success() {
            Ok(String::from_utf8(out.stdout.trim_ascii().to_vec())?)
        } else {
            Err(anyhow!(
                "Error: {}",
                String::from_utf8(out.stderr.trim_ascii().to_vec())?.as_str()
            ))
        }
    }

    fn list(&self) -> Result<ZFSList> {
        Ok(serde_json::from_str(&Self::run(
            "zfs",
            vec![
                "list".to_string(),
                "-j".to_string(),
                "--json-int".to_string(),
            ],
        )?)?)
    }

    fn destroy(&self, pool: &str, name: &str) -> Result<()> {
        Self::run(
            "zfs",
            vec![
                "destroy".to_string(),
                "-f".to_string(),
                format!("{}/{}", pool, name),
            ],
        )?;
        Ok(())
    }

    fn create_dataset(
        &self,
        pool: &str,
        name: &str,
        options: Option<CommandOptions>,
    ) -> Result<()> {
        let mut args = vec!["create".to_string(), format!("{}/{}", pool, name)];

        if let Some(options) = options {
            args.append(&mut options.to_options())
        }

        Self::run("zfs", args)?;
        Ok(())
    }

    fn rename(&self, pool: &str, orig: &str, new: &str) -> Result<()> {
        let args = vec![
            "rename",
            "-p",
            &format!("{}/{}", pool, orig),
            &format!("{}/{}", pool, new),
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        Self::run("zfs", args)?;
        Ok(())
    }

    fn set(&self, pool: &str, name: &str, properties: HashMap<&str, String>) -> Result<()> {
        if properties.is_empty() {
            return Ok(());
        }

        let mut args = vec!["set".to_string()];

        for (key, value) in &properties {
            args.push(format!("{}={}", key, value));
        }

        args.push(format!("{}/{}", pool, name));

        Self::run("zfs", args)?;
        Ok(())
    }

    fn get<T>(&self, pool: &str, name: &str, property: &str) -> Result<T>
    where
        T: for<'de> serde::Deserialize<'de> + FromStr + Send + Sync + Clone,
        T::Err: ToString,
    {
        let args = vec![
            "get".to_string(),
            "-j".to_string(),
            "--json-int".to_string(),
            property.to_string(),
            format!("{}/{}", pool, name),
        ];

        let out: ZFSGet<T> = serde_json::from_str(&Self::run("zfs", args)?)?;

        Ok(
            out.datasets[&format!("{}/{}", pool, name)].properties[property]
                .value
                .clone(),
        )
    }

    fn mount(&self, pool: &str) -> Result<()> {
        Self::run(
            "zfs",
            vec!["mount", "-R", pool]
                .iter()
                .map(|x| x.to_string())
                .collect::<Vec<String>>(),
        )?;
        Ok(())
    }

    fn unmount(&self, pool: &str, name: &str) -> Result<()> {
        Self::run(
            "zfs",
            vec!["unmount", "-f", &format!("{}/{}", pool, name)]
                .iter()
                .map(|x| x.to_string())
                .collect::<Vec<String>>(),
        )?;
        Ok(())
    }

    fn create_volume(
        &self,
        pool: &str,
        name: &str,
        size: u64, // 640k aughta be enough for anybody
        options: Option<CommandOptions>,
    ) -> Result<()> {
        let mut args = vec![
            "create".to_string(),
            "-V".to_string(),
            format!("{}", size),
            format!("{}/{}", pool, name),
        ];

        if let Some(options) = options {
            args.append(&mut options.to_options())
        }

        Self::run("zfs", args)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    mod controller {
        use super::super::Pool;
        use crate::{
            testutil::{create_zpool, destroy_zpool, BUCKLE_TEST_ZPOOL_PREFIX},
            zfs::{Dataset, ModifyDataset, ModifyVolume, Volume, ZFSKind},
        };
        #[test]
        fn test_controller_zfs_lifecycle() {
            let _ = destroy_zpool("controller-list", None);
            let file = create_zpool("controller-list").unwrap();
            let pool = Pool::new(&format!("{}-controller-list", BUCKLE_TEST_ZPOOL_PREFIX));
            let list = pool.list(None).unwrap();
            assert_eq!(list.len(), 0);
            pool.create_dataset(&crate::zfs::Dataset {
                name: "dataset".to_string(),
                quota: None,
            })
            .unwrap();
            let list = pool.list(None).unwrap();
            assert_eq!(list.len(), 1);
            assert_eq!(list[0].kind, ZFSKind::Dataset);
            assert_eq!(list[0].name, "dataset");
            assert_eq!(
                list[0].full_name,
                format!("{}-controller-list/dataset", BUCKLE_TEST_ZPOOL_PREFIX),
            );
            assert_ne!(list[0].size, 0);
            assert_ne!(list[0].used, 0);
            assert_ne!(list[0].refer, 0);
            assert_ne!(list[0].avail, 0);
            assert_eq!(
                list[0].mountpoint,
                Some(format!(
                    "/{}-controller-list/dataset",
                    BUCKLE_TEST_ZPOOL_PREFIX
                ))
            );
            pool.create_volume(&crate::zfs::Volume {
                name: "volume".to_string(),
                size: 100 * 1024 * 1024,
            })
            .unwrap();
            let list = pool.list(None).unwrap();
            assert_eq!(list.len(), 2);

            let list = pool.list(Some("volume".to_string())).unwrap();
            assert_eq!(list.len(), 1);
            assert_eq!(list[0].kind, ZFSKind::Volume);
            assert_eq!(list[0].name, "volume");
            assert_eq!(
                list[0].full_name,
                format!("{}-controller-list/volume", BUCKLE_TEST_ZPOOL_PREFIX),
            );
            assert_ne!(list[0].size, 0);
            assert_ne!(list[0].used, 0);
            assert_ne!(list[0].refer, 0);
            assert_ne!(list[0].avail, 0);
            assert_eq!(list[0].mountpoint, None);

            pool.modify_volume(ModifyVolume {
                name: "volume".into(),
                modifications: Volume {
                    name: "volume2".into(),
                    size: 150 * 1024 * 1024,
                },
            })
            .unwrap();

            let list = pool.list(Some("volume2".to_string())).unwrap();
            assert_eq!(list.len(), 1);
            assert_eq!(list[0].kind, ZFSKind::Volume);
            assert_eq!(list[0].name, "volume2");
            assert_eq!(
                list[0].full_name,
                format!("{}-controller-list/volume2", BUCKLE_TEST_ZPOOL_PREFIX),
            );
            assert_ne!(list[0].size, 0);
            assert!(
                list[0].size < 151 * 1024 * 1024 && list[0].size > 149 * 1024 * 1024,
                "{}",
                list[0].size
            );
            assert_ne!(list[0].used, 0);
            assert_ne!(list[0].refer, 0);
            assert_ne!(list[0].avail, 0);
            assert_eq!(list[0].mountpoint, None);

            let list = pool.list(Some("dataset".to_string())).unwrap();
            assert_eq!(list.len(), 1);
            assert_eq!(list[0].kind, ZFSKind::Dataset);
            assert_eq!(list[0].name, "dataset");
            assert_eq!(
                list[0].full_name,
                format!("{}-controller-list/dataset", BUCKLE_TEST_ZPOOL_PREFIX),
            );
            assert_ne!(list[0].size, 0);
            assert_ne!(list[0].used, 0);
            assert_ne!(list[0].refer, 0);
            assert_ne!(list[0].avail, 0);
            assert_eq!(
                list[0].mountpoint,
                Some(format!(
                    "/{}-controller-list/dataset",
                    BUCKLE_TEST_ZPOOL_PREFIX
                ))
            );

            pool.modify_dataset(ModifyDataset {
                name: "dataset".into(),
                modifications: Dataset {
                    name: "dataset2".into(),
                    quota: Some(5 * 1024 * 1024),
                },
            })
            .unwrap();

            let list = pool.list(Some("dataset2".to_string())).unwrap();
            assert_eq!(list.len(), 1);
            assert_eq!(list[0].kind, ZFSKind::Dataset);
            assert_eq!(list[0].name, "dataset2");
            assert_eq!(
                list[0].full_name,
                format!("{}-controller-list/dataset2", BUCKLE_TEST_ZPOOL_PREFIX),
            );
            assert_ne!(list[0].size, 0);
            assert_ne!(list[0].used, 0);
            assert_ne!(list[0].refer, 0);
            assert_ne!(list[0].avail, 0);
            assert_eq!(
                list[0].mountpoint,
                Some(format!(
                    "/{}-controller-list/dataset2",
                    BUCKLE_TEST_ZPOOL_PREFIX
                ))
            );

            pool.destroy("dataset2".to_string()).unwrap();
            let list = pool.list(Some("dataset2".to_string())).unwrap();
            assert_eq!(list.len(), 0);
            let list = pool.list(None).unwrap();
            assert_eq!(list.len(), 1);
            pool.destroy("volume2".to_string()).unwrap();
            let list = pool.list(Some("volume2".to_string())).unwrap();
            assert_eq!(list.len(), 0);
            let list = pool.list(None).unwrap();
            assert_eq!(list.len(), 0);
            destroy_zpool("controller-list", Some(&file)).unwrap();
        }
    }
}
