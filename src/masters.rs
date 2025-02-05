use serde_derive::{Serialize,Deserialize};
use chrono::{DateTime, Local};
use std::{fs, process, sync::mpsc::channel, time::Instant, env, error::Error};
use std::collections::BTreeMap;
use log::*;
use colored::*;
use crate::isleader::AllStoredIsLeader;
use crate::utility::{scan_host_port, http_get};

#[derive(Serialize, Deserialize, Debug)]
pub struct AllMasters {
    pub masters: Vec<Masters>,
}

#[derive(Default)]
pub struct AllStoredMasters {
    stored_masters: Vec<StoredMasters>,
    stored_rpc_addresses: Vec<StoredRpcAddresses>,
    stored_http_addresses: Vec<StoredHttpAddresses>,
    stored_master_error: Vec<StoredMasterError>,
}

impl AllStoredMasters {
    pub async fn perform_snapshot(
        hosts: &Vec<&str>,
        ports: &Vec<&str>,
        snapshot_number: i32,
        parallel: usize,
    ) {
        info!("begin snapshot");
        let timer = Instant::now();

        let allmasters = AllStoredMasters::read_masters(hosts, ports, parallel);
        allmasters.await.save_snapshot(snapshot_number)
            .unwrap_or_else(|e| {
                error!("error saving snapshot: {}", e);
                process::exit(1);
            });

        info!("end snapshot: {:?}", timer.elapsed())
    }
    pub fn new() -> Self {
        Default::default()
    }
    pub async fn read_masters(
        hosts: &Vec<&str>,
        ports: &Vec<&str>,
        parallel: usize,
    ) -> AllStoredMasters
    {
        info!("begin parallel http read");
        let timer = Instant::now();

        let pool = rayon::ThreadPoolBuilder::new().num_threads(parallel).build().unwrap();
        let (tx, rx) = channel();
        pool.scope(move |s| {
            for host in hosts {
                for port in ports {
                    let tx = tx.clone();
                    s.spawn(move |_| {
                        let detail_snapshot_time = Local::now();
                        let masters = AllStoredMasters::read_http(host, port);
                        tx.send((format!("{}:{}", host, port), detail_snapshot_time, masters)).expect("error sending data via tx (masters)");
                    });
                }
            }
        });

        info!("end parallel http read {:?}", timer.elapsed());

        let mut allstoredmasters = AllStoredMasters::new();

        for (hostname_port, detail_snapshot_time, masters) in rx {
            allstoredmasters.split_into_vectors(masters, &hostname_port, detail_snapshot_time);
        }

        allstoredmasters
    }
    fn split_into_vectors(
        &mut self,
        masters: AllMasters,
        hostname_port: &str,
        detail_snapshot_time: DateTime<Local>,
    )
    {
        for master in masters.masters {
            let mut placement_cloud = String::from("Unset");
            let mut placement_region = String::from("Unset");
            let mut placement_zone = String::from("Unset");
            if let Some(cloud_info) = master.registration.cloud_info {
                placement_cloud = cloud_info.placement_cloud;
                placement_region = cloud_info.placement_region;
                placement_zone = cloud_info.placement_zone;
            };
            self.stored_masters.push( StoredMasters {
                hostname_port: hostname_port.to_string(),
                timestamp: detail_snapshot_time,
                instance_permanent_uuid: master.instance_id.permanent_uuid.to_string(),
                instance_instance_seqno: master.instance_id.instance_seqno,
                start_time_us: master.instance_id.start_time_us.unwrap_or_default(),
                registration_cloud_placement_cloud: placement_cloud.to_string(),
                registration_cloud_placement_region: placement_region.to_string(),
                registration_cloud_placement_zone: placement_zone.to_string(),
                registration_placement_uuid: master.registration.placement_uuid.unwrap_or_else(|| "Unset".to_string()).to_string(),
                role: master.role.unwrap_or_else(|| "Unnknown".to_string()).to_string(),
            });
            if let Some(error) = master.error {
                self.stored_master_error.push(StoredMasterError {
                    hostname_port: hostname_port.to_string(),
                    timestamp: detail_snapshot_time,
                    instance_permanent_uuid: master.instance_id.permanent_uuid.to_string(),
                    code: error.code.to_string(),
                    message: error.message.to_string(),
                    posix_code: error.posix_code,
                    source_file: error.source_file.to_string(),
                    source_line: error.source_line,
                    errors: error.errors.to_string(),
                });
            }
            for rpc_address in master.registration.private_rpc_addresses {
                self.stored_rpc_addresses.push( StoredRpcAddresses {
                    hostname_port: hostname_port.to_string(),
                    timestamp: detail_snapshot_time,
                    instance_permanent_uuid: master.instance_id.permanent_uuid.to_string(),
                    host: rpc_address.host.to_string(),
                    port: rpc_address.port.to_string(),
                });
            };
            if let Some(http_addresses) = master.registration.http_addresses {
                for http_address in http_addresses {
                    self.stored_http_addresses.push(StoredHttpAddresses {
                        hostname_port: hostname_port.to_string(),
                        timestamp: detail_snapshot_time,
                        instance_permanent_uuid: master.instance_id.permanent_uuid.to_string(),
                        host: http_address.host.to_string(),
                        port: http_address.port.to_string(),
                    });
                };
            };
        }
    }
    pub fn read_http(
        host: &str,
        port: &str,
    ) -> AllMasters
    {
        let data_from_http = if scan_host_port( host, port) {
            http_get(host, port, "api/v1/masters")
        } else {
            String::new()
        };
        AllStoredMasters::parse_masters(data_from_http, host, port)
        /*
        if ! scan_port_addr(format!("{}:{}", host, port)) {
            warn!("hostname: port {}:{} cannot be reached, skipping", host, port);
            return AllStoredMasters::parse_masters(String::from(""), "", "")
        };
        let data_from_http = reqwest::blocking::get(format!("http://{}:{}/api/v1/masters", host, port))
            .unwrap_or_else(|e| {
                error!("Fatal: error reading from URL: {}", e);
                process::exit(1);
            })
            .text().unwrap();

        let data_from_http = reqwest::blocking::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap()
            .get(format!("http://{}:{}/api/v1/masters", host, port))
            .send()
            .unwrap()
            .text()
            .unwrap();
         */

    }
    fn parse_masters(
        masters_data: String,
        host: &str,
        port: &str,
    ) -> AllMasters {
        serde_json::from_str(&masters_data )
            .unwrap_or_else(|e| {
                debug!("({}:{}) could not parse /api/v1/masters json data for masters, error: {}", host, port, e);
                AllMasters { masters: Vec::<Masters>::new() }
            })
    }
    fn save_snapshot ( self, snapshot_number: i32 ) -> Result<(), Box<dyn Error>>
    {
        let current_directory = env::current_dir()?;
        let current_snapshot_directory = current_directory.join("yb_stats.snapshots").join(&snapshot_number.to_string());

        let masters_file = &current_snapshot_directory.join("masters");
        let file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .open(masters_file)?;
        let mut writer = csv::Writer::from_writer(file);
        for row in self.stored_masters {
            writer.serialize(row)?;
        }
        writer.flush()?;

        let master_rpc_addresses_file = &current_snapshot_directory.join("master_rpc_addresses");
        let file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .open(master_rpc_addresses_file)?;
        let mut writer = csv::Writer::from_writer(file);
        for row in self.stored_rpc_addresses {
            writer.serialize(row)?;
        }
        writer.flush()?;

        let master_http_addresses_file = &current_snapshot_directory.join("master_http_addresses");
        let file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .open(master_http_addresses_file)?;
        let mut writer = csv::Writer::from_writer(file);
        for row in self.stored_http_addresses {
            writer.serialize(row)?;
        }
        writer.flush()?;

        let master_errors_file = &current_snapshot_directory.join("master_errors");
        let file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .open(master_errors_file)?;
        let mut writer = csv::Writer::from_writer(file);
        for row in self.stored_master_error {
            writer.serialize(row)?;
        }
        writer.flush()?;

        Ok(())
    }
    pub fn read_snapshot( snapshot_number: &String, ) -> Result<AllStoredMasters, Box<dyn Error>>
    {
        let mut allstoredmasters = AllStoredMasters {
            stored_masters: Vec::new(),
            stored_rpc_addresses: Vec::new(),
            stored_http_addresses: Vec::new(),
            stored_master_error: Vec::new(),
        };

        let current_directory = env::current_dir()?;
        let current_snapshot_directory = current_directory.join("yb_stats.snapshots").join(snapshot_number);

        let masters_file = &current_snapshot_directory.join("masters");
        let file = fs::File::open(masters_file)?;

        let mut reader = csv::Reader::from_reader(file);
        for row in reader.deserialize() {
            let data: StoredMasters = row?;
            allstoredmasters.stored_masters.push(data);
        };

        let masters_rpc_addresses_file = &current_snapshot_directory.join("master_rpc_addresses");
        let file = fs::File::open(masters_rpc_addresses_file)?;

        let mut reader = csv::Reader::from_reader(file);
        for row in reader.deserialize() {
            let data: StoredRpcAddresses = row?;
            allstoredmasters.stored_rpc_addresses.push(data);
        };

        let masters_http_addresses_file = &current_snapshot_directory.join("master_http_addresses");
        let file = fs::File::open(masters_http_addresses_file)?;

        let mut reader = csv::Reader::from_reader(file);
        for row in reader.deserialize() {
            let data: StoredHttpAddresses = row?;
            allstoredmasters.stored_http_addresses.push(data);
        };

        let masters_error_file = &current_snapshot_directory.join("master_errors");
        let file = fs::File::open(masters_error_file)?;

        let mut reader = csv::Reader::from_reader(file);
        for row in reader.deserialize() {
            let data: StoredMasterError = row?;
            allstoredmasters.stored_master_error.push(data);
        };

        Ok(allstoredmasters)
    }
    pub fn print(
        &self,
        snapshot_number: &String,
        details_enable: &bool,
    )
    {
        info!("print masters");

        let leader_hostname = AllStoredIsLeader::return_leader_snapshot(snapshot_number);

        for row in &self.stored_masters {
            if row.hostname_port == leader_hostname
            && !*details_enable {
                println!("{} {:8} Cloud: {}, Region: {}, Zone: {}", row.instance_permanent_uuid, row.role, row.registration_cloud_placement_cloud, row.registration_cloud_placement_region, row.registration_cloud_placement_zone);
                println!("{} Seqno: {} Start time: {}", " ".repeat(32), row.instance_instance_seqno, row.start_time_us);
                print!("{} RPC addresses: ( ", " ".repeat(32));
                for rpc_address in self.stored_rpc_addresses.iter()
                    .filter(|x| x.hostname_port == row.hostname_port)
                    .filter(|x| x.instance_permanent_uuid == row.instance_permanent_uuid) {
                    print!("{}:{} ", rpc_address.host, rpc_address.port);
                };
                println!(" )");
                print!("{} HTTP addresses: ( ", " ".repeat(32));
                for http_address in self.stored_http_addresses.iter()
                    .filter(|x| x.hostname_port == row.hostname_port)
                    .filter(|x| x.instance_permanent_uuid == row.instance_permanent_uuid) {
                    print!("{}:{} ", http_address.host, http_address.port);
                };
                println!(" )");
                for error in self.stored_master_error.iter()
                    .filter(|x| x.hostname_port == row.hostname_port)
                    .filter(|x| x.instance_permanent_uuid == row.instance_permanent_uuid) {
                    println!("{:#?}", error);
                };
            }
            if *details_enable {
                println!("{} {} {:8} Cloud: {}, Region: {}, Zone: {}", row.hostname_port, row.instance_permanent_uuid, row.role, row.registration_cloud_placement_cloud, row.registration_cloud_placement_region, row.registration_cloud_placement_zone);
                println!("{} {} Seqno: {} Start time: {}", row.hostname_port, " ".repeat(32), row.instance_instance_seqno, row.start_time_us);
                print!("{} {} RPC addresses: ( ", row.hostname_port, " ".repeat(32));
                for rpc_address in self.stored_rpc_addresses.iter()
                    .filter(|x| x.hostname_port == row.hostname_port)
                    .filter(|x| x.instance_permanent_uuid == row.instance_permanent_uuid) {
                    print!("{}:{} ", rpc_address.host, rpc_address.port);
                };
                println!(" )");
                print!("{} {} HTTP addresses: ( ", row.hostname_port, " ".repeat(32));
                for http_address in self.stored_http_addresses.iter()
                    .filter(|x| x.hostname_port == row.hostname_port)
                    .filter(|x| x.instance_permanent_uuid == row.instance_permanent_uuid) {
                    print!("{}:{} ", http_address.host, http_address.port);
                };
                println!(" )");
                for error in self.stored_master_error.iter()
                    .filter(|x| x.hostname_port == row.hostname_port)
                    .filter(|x| x.instance_permanent_uuid == row.instance_permanent_uuid) {
                    println!("{} {:#?}", row.hostname_port, error);
                };
            }
        }
    }
    pub async fn print_adhoc(
        &self,
        details_enable: &bool,
        hosts: &Vec<&str>,
        ports: &Vec<&str>,
        parallel: usize,
    )
    {
        info!("print adhoc masters");

        let leader_hostname = AllStoredIsLeader::return_leader_http(hosts, ports, parallel).await;

        for row in &self.stored_masters {
            if row.hostname_port == leader_hostname
                && !*details_enable {
                println!("{} {:8} Cloud: {}, Region: {}, Zone: {}", row.instance_permanent_uuid, row.role, row.registration_cloud_placement_cloud, row.registration_cloud_placement_region, row.registration_cloud_placement_zone);
                println!("{} Seqno: {} Start time: {}", " ".repeat(32), row.instance_instance_seqno, row.start_time_us);
                print!("{} RPC addresses: ( ", " ".repeat(32));
                for rpc_address in self.stored_rpc_addresses.iter()
                    .filter(|x| x.hostname_port == row.hostname_port)
                    .filter(|x| x.instance_permanent_uuid == row.instance_permanent_uuid) {
                    print!("{}:{} ", rpc_address.host, rpc_address.port);
                };
                println!(" )");
                print!("{} HTTP addresses: ( ", " ".repeat(32));
                for http_address in self.stored_http_addresses.iter()
                    .filter(|x| x.hostname_port == row.hostname_port)
                    .filter(|x| x.instance_permanent_uuid == row.instance_permanent_uuid) {
                    print!("{}:{} ", http_address.host, http_address.port);
                };
                println!(" )");
                for error in self.stored_master_error.iter()
                    .filter(|x| x.hostname_port == row.hostname_port)
                    .filter(|x| x.instance_permanent_uuid == row.instance_permanent_uuid) {
                    println!("{:#?}", error);
                };
            }
            if *details_enable {
                println!("{} {} {:8} Cloud: {}, Region: {}, Zone: {}", row.hostname_port, row.instance_permanent_uuid, row.role, row.registration_cloud_placement_cloud, row.registration_cloud_placement_region, row.registration_cloud_placement_zone);
                println!("{} {} Seqno: {} Start time: {}", row.hostname_port, " ".repeat(32), row.instance_instance_seqno, row.start_time_us);
                print!("{} {} RPC addresses: ( ", row.hostname_port, " ".repeat(32));
                for rpc_address in self.stored_rpc_addresses.iter()
                    .filter(|x| x.hostname_port == row.hostname_port)
                    .filter(|x| x.instance_permanent_uuid == row.instance_permanent_uuid) {
                    print!("{}:{} ", rpc_address.host, rpc_address.port);
                };
                println!(" )");
                print!("{} {} HTTP addresses: ( ", row.hostname_port, " ".repeat(32));
                for http_address in self.stored_http_addresses.iter()
                    .filter(|x| x.hostname_port == row.hostname_port)
                    .filter(|x| x.instance_permanent_uuid == row.instance_permanent_uuid) {
                    print!("{}:{} ", http_address.host, http_address.port);
                };
                println!(" )");
                for error in self.stored_master_error.iter()
                    .filter(|x| x.hostname_port == row.hostname_port)
                    .filter(|x| x.instance_permanent_uuid == row.instance_permanent_uuid) {
                    println!("{} {:#?}", row.hostname_port, error);
                };
            }
        }
    }
}

#[derive(Debug)]
pub struct SnapshotDiffStoredMasters {
    pub first_instance_seqno: i64,
    pub first_start_time_us: i64,
    pub first_registration_cloud_placement_cloud: String,
    pub first_registration_cloud_placement_region: String,
    pub first_registration_cloud_placement_zone: String,
    pub first_registration_placement_uuid: String,
    pub first_role: String,
    pub second_instance_seqno: i64,
    pub second_start_time_us: i64,
    pub second_registration_cloud_placement_cloud: String,
    pub second_registration_cloud_placement_region: String,
    pub second_registration_cloud_placement_zone: String,
    pub second_registration_placement_uuid: String,
    pub second_role: String,
}
impl SnapshotDiffStoredMasters {
    fn first_snapshot( storedmasters: StoredMasters ) -> Self
    {
        Self {
            first_instance_seqno: storedmasters.instance_instance_seqno,
            first_start_time_us: storedmasters.start_time_us,
            first_registration_cloud_placement_cloud: storedmasters.registration_cloud_placement_cloud.to_string(),
            first_registration_cloud_placement_region: storedmasters.registration_cloud_placement_region.to_string(),
            first_registration_cloud_placement_zone: storedmasters.registration_cloud_placement_zone.to_string(),
            first_registration_placement_uuid: storedmasters.registration_placement_uuid.to_string(),
            first_role: storedmasters.role,
            second_instance_seqno: 0,
            second_start_time_us: 0,
            second_registration_cloud_placement_cloud: "".to_string(),
            second_registration_cloud_placement_region: "".to_string(),
            second_registration_cloud_placement_zone: "".to_string(),
            second_registration_placement_uuid: "".to_string(),
            second_role: "".to_string(),
        }
    }
    fn second_snapshot_new( storedmasters: StoredMasters ) -> Self
    {
        Self {
            first_instance_seqno: 0,
            first_start_time_us: 0,
            first_registration_cloud_placement_cloud: "".to_string(),
            first_registration_cloud_placement_region: "".to_string(),
            first_registration_cloud_placement_zone: "".to_string(),
            first_registration_placement_uuid: "".to_string(),
            first_role: "".to_string(),
            second_instance_seqno: storedmasters.instance_instance_seqno,
            second_start_time_us: storedmasters.start_time_us,
            second_registration_cloud_placement_cloud: storedmasters.registration_cloud_placement_cloud.to_string(),
            second_registration_cloud_placement_region: storedmasters.registration_cloud_placement_region.to_string(),
            second_registration_cloud_placement_zone: storedmasters.registration_cloud_placement_zone.to_string(),
            second_registration_placement_uuid: storedmasters.registration_placement_uuid.to_string(),
            second_role: storedmasters.role,
        }
    }
    fn second_snapshot_existing( storedmasters_diff_row: &mut SnapshotDiffStoredMasters, storedmasters: StoredMasters ) -> Self
    {
        Self {
            first_instance_seqno: storedmasters_diff_row.first_instance_seqno,
            first_start_time_us: storedmasters_diff_row.first_start_time_us,
            first_registration_cloud_placement_cloud: storedmasters_diff_row.first_registration_cloud_placement_cloud.to_string(),
            first_registration_cloud_placement_region: storedmasters_diff_row.first_registration_cloud_placement_region.to_string(),
            first_registration_cloud_placement_zone: storedmasters_diff_row.first_registration_cloud_placement_zone.to_string(),
            first_registration_placement_uuid: storedmasters_diff_row.first_registration_placement_uuid.to_string(),
            first_role: storedmasters_diff_row.first_role.to_string(),
            second_instance_seqno: storedmasters.instance_instance_seqno,
            second_start_time_us: storedmasters.start_time_us,
            second_registration_cloud_placement_cloud: storedmasters.registration_cloud_placement_cloud.to_string(),
            second_registration_cloud_placement_region: storedmasters.registration_cloud_placement_region.to_string(),
            second_registration_cloud_placement_zone: storedmasters.registration_cloud_placement_zone.to_string(),
            second_registration_placement_uuid: storedmasters.registration_placement_uuid.to_string(),
            second_role: storedmasters.role,
        }
    }
}
type BTreeMapSnapshotDiffMasters = BTreeMap<String, SnapshotDiffStoredMasters>;
type BTreeMapSnapshotDiffHttpAddresses = BTreeMap<(String, String), SnapshotDiffHttpAddresses>;
type BTreeMapSnapshotDiffRpcAddresses = BTreeMap<(String, String), SnapshotDiffRpcAddresses>;
#[derive(Debug)]
pub struct PermanentUuidHttpAddress {
    pub permanent_uuid: String,
    pub hostname_port: String,
}
#[derive(Debug)]
pub struct PermanentUuidRpcAddress {
    pub permanent_uuid: String,
    pub hostname_port: String,
}

#[derive(Default)]
pub struct SnapshotDiffBTreeMapsMasters {
    pub btreemap_snapshotdiff_masters: BTreeMapSnapshotDiffMasters,
    pub btreemap_snapshotdiff_httpaddresses: BTreeMapSnapshotDiffHttpAddresses, // currently no diff for http addresses
    pub btreemap_snapshotdiff_rpcaddresses: BTreeMapSnapshotDiffRpcAddresses, // currently no diff for rpc addresses
    pub first_http_addresses: Vec<PermanentUuidHttpAddress>,
    pub second_http_addresses: Vec<PermanentUuidHttpAddress>,
    pub first_rpc_addresses: Vec<PermanentUuidRpcAddress>,
    pub second_rpc_addresses: Vec<PermanentUuidRpcAddress>,
    pub master_found: bool,
}

impl SnapshotDiffBTreeMapsMasters {
    pub fn snapshot_diff(
        begin_snapshot: &String,
        end_snapshot: &String,
    ) -> SnapshotDiffBTreeMapsMasters
    {
        let allstoredmasters = AllStoredMasters::read_snapshot(begin_snapshot)
            .unwrap_or_else(|e| {
                error!("Fatal: error reading snapshot: {}", e);
                process::exit(1);
            });
        let master_leader = AllStoredIsLeader::return_leader_snapshot(begin_snapshot);
        let mut masters_snapshot_diff = SnapshotDiffBTreeMapsMasters::new();
        masters_snapshot_diff.first_snapshot(allstoredmasters, master_leader);

        let allstoredmasters = AllStoredMasters::read_snapshot(end_snapshot)
            .unwrap_or_else(|e| {
                error!("Fatal: error reading snapshot: {}", e);
                process::exit(1);
            });
        let master_leader = AllStoredIsLeader::return_leader_snapshot(begin_snapshot);
        masters_snapshot_diff.second_snapshot(allstoredmasters, master_leader);

        masters_snapshot_diff
    }
    pub fn new() -> Self {
        Default::default()
    }
    fn first_snapshot(
        &mut self,
        allstoredmasters: AllStoredMasters,
        master_leader: String,
    )
    {
        if master_leader == *"" {
            self.master_found = false;
            return;
        } else {
            self.master_found = true;
        };
        trace!("first snapshot: master_leader: {}, found: {}", master_leader, self.master_found);

        for row in allstoredmasters.stored_masters.into_iter().filter(|r| r.hostname_port == master_leader.clone()) {
            match self.btreemap_snapshotdiff_masters.get_mut( &row.instance_permanent_uuid ) {
                Some( _master_row ) => {
                    error!("Found second entry for first entry of masters based on instance permanent uuid: {}", &row.instance_permanent_uuid);
                },
                None => {
                    trace!("first snapshot: add master permanent uuid: {}", row.instance_permanent_uuid.to_string() );
                    self.btreemap_snapshotdiff_masters.insert(
                        row.instance_permanent_uuid.to_string(),
                        SnapshotDiffStoredMasters::first_snapshot(row)
                    );
                },
            };
        }
        for row in allstoredmasters.stored_http_addresses.into_iter().filter(|r| r.hostname_port == master_leader.clone()) {
            if self.first_http_addresses.iter().filter(|r| r.permanent_uuid == row.instance_permanent_uuid && r.hostname_port == format!("{}:{}", row.host, row.port)).count() == 0 {
                trace!("first snapshot: add http address: {}:{}", row.host.to_string(), row.port.to_string() );
                self.first_http_addresses.push( PermanentUuidHttpAddress { permanent_uuid: row.instance_permanent_uuid.to_string(), hostname_port: format!("{}:{}", row.host, row.port) });
            }
        }
        for row in allstoredmasters.stored_rpc_addresses.into_iter().filter(|r| r.hostname_port == master_leader.clone()) {
            if self.first_rpc_addresses.iter().filter(|r| r.permanent_uuid == row.instance_permanent_uuid && r.hostname_port == format!("{}:{}", row.host, row.port)).count() == 0 {
                trace!("first snapshot: add rpc address: {}:{}", row.host.to_string(), row.port.to_string() );
                self.first_rpc_addresses.push( PermanentUuidRpcAddress { permanent_uuid: row.instance_permanent_uuid.to_string(), hostname_port: format!("{}:{}", row.host, row.port) });
            }
        }
    }
    fn second_snapshot(
        &mut self,
        allstoredmasters: AllStoredMasters,
        master_leader: String,
    )
    {
        if master_leader == *"" {
            self.master_found = false;
            return;
        } else {
            self.master_found = true;
        };
        trace!("second snapshot: master_leader: {}, found: {}", master_leader, self.master_found);

        for row in allstoredmasters.stored_masters.into_iter().filter(|r| r.hostname_port == master_leader.clone()) {
            match self.btreemap_snapshotdiff_masters.get_mut( &row.instance_permanent_uuid.clone() ) {
                Some( master_row) => {
                    if master_row.first_instance_seqno == row.instance_instance_seqno
                        && master_row.first_start_time_us == row.start_time_us
                        && master_row.first_registration_cloud_placement_cloud == row.registration_cloud_placement_cloud
                        && master_row.first_registration_cloud_placement_region == row.registration_cloud_placement_region
                        && master_row.first_registration_cloud_placement_zone == row.registration_cloud_placement_zone
                        && master_row.first_role == row.role
                    {
                        // the second snapshot contains identicial values, so we remove it.
                        trace!("second snapshot: idential:remove: {}", row.instance_permanent_uuid.to_string() );
                        self.btreemap_snapshotdiff_masters.remove( &row.instance_permanent_uuid.clone() );
                    }
                    else {
                        trace!("second snapshot: CHANGED: {}", row.instance_permanent_uuid.to_string() );
                        *master_row = SnapshotDiffStoredMasters::second_snapshot_existing(master_row, row);
                    }
                },
                None => {
                    trace!("second snapshot: new: {}", row.instance_permanent_uuid.to_string() );
                    self.btreemap_snapshotdiff_masters.insert( row.instance_permanent_uuid.clone(), SnapshotDiffStoredMasters::second_snapshot_new(row));
                }
            }
        }
        for row in allstoredmasters.stored_http_addresses.into_iter().filter(|r| r.hostname_port == master_leader.clone()) {
            if self.second_http_addresses.iter().filter(|r| r.permanent_uuid == row.instance_permanent_uuid && r.hostname_port == format!("{}:{}", row.host, row.port)).count() == 0 {
                trace!("second snapshot: new http address: {}:{}", row.host.to_string(), row.port.to_string() );
                self.second_http_addresses.push( PermanentUuidHttpAddress { permanent_uuid: row.instance_permanent_uuid.to_string(), hostname_port: format!("{}:{}", row.host, row.port) });
            }
        }
        for row in allstoredmasters.stored_rpc_addresses.into_iter().filter(|r| r.hostname_port == master_leader.clone()) {
            if self.second_rpc_addresses.iter().filter(|r| r.permanent_uuid == row.instance_permanent_uuid && r.hostname_port == format!("{}:{}", row.host, row.port)).count() == 0 {
                trace!("second snapshot: new rpc address: {}:{}", row.host.to_string(), row.port.to_string() );
                self.second_rpc_addresses.push( PermanentUuidRpcAddress { permanent_uuid: row.instance_permanent_uuid.to_string(), hostname_port: format!("{}:{}", row.host, row.port) });
            }
        }
    }
    pub fn print(
        &self,
    )
    {
        if ! self.master_found {
            println!("Master leader was not found in hosts specified, skipping masters diff.");
            return;
        }
        for (permanent_uuid, row) in self.btreemap_snapshotdiff_masters.iter() {
            if row.second_instance_seqno == 0 {
                // If the second instance_seqno is zero, it means the permanent_uuid is gone. This means the master is gone.
                println!("{} Master {} {:8} Cloud: {}, Region: {}, Zone: {}", "-".to_string().red(), permanent_uuid, row.first_role, row.first_registration_cloud_placement_cloud, row.first_registration_cloud_placement_region, row.first_registration_cloud_placement_zone);
                println!("         {} Seqno: {} Start time: {}", " ".repeat(32), row.first_instance_seqno, row.first_start_time_us);
                print!("         {} Http ( ", " ".repeat(32));
                for http_address in self.first_http_addresses.iter().filter(|r| r.permanent_uuid == permanent_uuid.clone()) {
                    print!("{} ", http_address.hostname_port);
                }
                println!(")");
                print!("         {} Rpc ( ", " ".repeat(32));
                for http_address in self.first_rpc_addresses.iter().filter(|r| r.permanent_uuid == permanent_uuid.clone()) {
                    print!("{} ", http_address.hostname_port);
                }
                println!(")");
            } else if row.first_instance_seqno == 0 {
                // if the first instance_seqno is zero, it means the permanent_uuid has appeared after the first snapshot. This means it's a new master.
                println!("{} Master {} {:8} Cloud: {}, Region: {}, Zone: {}", "+".to_string().green(), permanent_uuid, row.second_role, row.second_registration_cloud_placement_cloud, row.second_registration_cloud_placement_region, row.second_registration_cloud_placement_zone);
                println!("         {} Seqno: {} Start time: {}", " ".repeat(32), row.second_instance_seqno, row.second_start_time_us);
                print!("         {} Http ( ", " ".repeat(32));
                for http_address in self.second_http_addresses.iter().filter(|r| r.permanent_uuid == permanent_uuid.clone()) {
                    print!("{} ", http_address.hostname_port);
                }
                println!(")");
                print!("         {} Rpc ( ", " ".repeat(32));
                for http_address in self.second_rpc_addresses.iter().filter(|r| r.permanent_uuid == permanent_uuid.clone()) {
                    print!("{} ", http_address.hostname_port);
                }
                println!(")");
            } else {
                // If both instance_seqno's have a number for the same permanent_uuid, it means we found something changed for a master.
                print!("{} Master {} ", "*".to_string().yellow(), permanent_uuid);
                if row.first_role != row.second_role {
                    print!("{}->{} ",row.first_role.to_string().yellow(), row.second_role.to_string().yellow());
                } else {
                    print!("{} ", row.second_role)
                };
                if row.first_registration_cloud_placement_cloud != row.second_registration_cloud_placement_cloud {
                    print!("Cloud: {}->{}, ",row.first_registration_cloud_placement_cloud.to_string().yellow(), row.second_registration_cloud_placement_cloud.to_string().yellow());
                } else {
                    print!("Cloud: {}, ", row.second_registration_cloud_placement_cloud)
                };
                if row.first_registration_cloud_placement_region != row.second_registration_cloud_placement_region {
                    print!("Region: {}->{}, ",row.first_registration_cloud_placement_region.to_string().yellow(), row.second_registration_cloud_placement_region.to_string().yellow());
                } else {
                    print!("Region: {}, ", row.second_registration_cloud_placement_region)
                };
                if row.first_registration_cloud_placement_zone != row.second_registration_cloud_placement_zone {
                    println!("Zone: {}->{}, ",row.first_registration_cloud_placement_zone.to_string().yellow(), row.second_registration_cloud_placement_zone.to_string().yellow());
                } else {
                    println!("Zone: {}, ", row.second_registration_cloud_placement_zone)
                };
                print!("         {} ", " ".repeat(32));
                if row.first_instance_seqno != row.second_instance_seqno {
                    print!("Seqno: {}->{}, ", row.first_instance_seqno.to_string().yellow(), row.second_instance_seqno.to_string().yellow());
                } else {
                    print!("Seqno: {}, ", row.second_instance_seqno);
                };
                if row.first_start_time_us != row.second_start_time_us {
                    println!("Start time: {}->{} ", row.first_start_time_us.to_string().yellow(), row.second_start_time_us.to_string().yellow());
                } else {
                    println!("Start time: {} ", row.second_start_time_us);
                };
                print!("         {} Http ( ", " ".repeat(32));
                for http_address in self.second_http_addresses.iter().filter(|r| r.permanent_uuid == permanent_uuid.clone()) {
                    print!("{} ", http_address.hostname_port);
                }
                println!(")");
                print!("         {} Rpc ( ", " ".repeat(32));
                for http_address in self.second_rpc_addresses.iter().filter(|r| r.permanent_uuid == permanent_uuid.clone()) {
                    print!("{} ", http_address.hostname_port);
                }
                println!(")");
            };
        };
    }
    pub async fn adhoc_read_first_snapshot(
        &mut self,
        hosts: &Vec<&str>,
        ports: &Vec<&str>,
        parallel: usize,
    )
    {
        let allstoredmasters = AllStoredMasters::read_masters(hosts, ports, parallel).await;
        let master_leader= AllStoredIsLeader::return_leader_http(hosts, ports, parallel).await;
        self.first_snapshot(allstoredmasters, master_leader);
    }
    pub async fn adhoc_read_second_snapshot(
        &mut self,
        hosts: &Vec<&str>,
        ports: &Vec<&str>,
        parallel: usize,
    )
    {
        let allstoredmasters = AllStoredMasters::read_masters(hosts, ports, parallel).await;
        let master_leader= AllStoredIsLeader::return_leader_http(hosts, ports, parallel).await;
        self.second_snapshot(allstoredmasters, master_leader);
    }
}
#[derive(Debug)]
pub struct SnapshotDiffRpcAddresses {
    pub first_port: String,
    pub second_port: String,
}
#[derive(Debug)]
pub struct SnapshotDiffHttpAddresses {
    pub first_port: String,
    pub second_port: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Masters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<MasterError>,
    pub instance_id: InstanceId,
    pub registration: Registration,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MasterError {
    pub code: String,
    pub message: String,
    pub posix_code: i32,
    pub source_file: String,
    pub source_line: i32,
    pub errors: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct InstanceId {
    pub instance_seqno: i64,
    pub permanent_uuid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_time_us: Option<i64>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Registration {
    pub private_rpc_addresses: Vec<PrivateRpcAddresses>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_addresses: Option<Vec<HttpAddresses>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cloud_info: Option<CloudInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placement_uuid: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PrivateRpcAddresses {
    pub host: String,
    pub port: i32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct HttpAddresses {
    pub host: String,
    pub port: i32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CloudInfo {
    pub placement_cloud: String,
    pub placement_region: String,
    pub placement_zone: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct StoredMasters {
    pub hostname_port: String,
    pub timestamp: DateTime<Local>,
    pub instance_permanent_uuid: String,
    pub instance_instance_seqno: i64,
    pub start_time_us: i64,
    pub registration_cloud_placement_cloud: String,
    pub registration_cloud_placement_region: String,
    pub registration_cloud_placement_zone: String,
    pub registration_placement_uuid: String,
    pub role: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct StoredRpcAddresses {
    pub hostname_port: String,
    pub timestamp: DateTime<Local>,
    pub instance_permanent_uuid: String,
    pub host: String,
    pub port: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct StoredHttpAddresses {
    pub hostname_port: String,
    pub timestamp: DateTime<Local>,
    pub instance_permanent_uuid: String,
    pub host: String,
    pub port: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct StoredMasterError {
    pub hostname_port: String,
    pub timestamp: DateTime<Local>,
    pub instance_permanent_uuid: String,
    pub code: String,
    pub message: String,
    pub posix_code: i32,
    pub source_file: String,
    pub source_line: i32,
    pub errors: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_parse_master_data() {
        let json = r#"
{
  "masters": [
    {
      "instance_id": {
        "permanent_uuid": "3fc1141619304cffa2f0a345d37a51c2",
        "instance_seqno": 1657972299220554,
        "start_time_us": 1657972299220554
      },
      "registration": {
        "private_rpc_addresses": [
          {
            "host": "yb-1.local",
            "port": 7100
          }
        ],
        "http_addresses": [
          {
            "host": "yb-1.local",
            "port": 7000
          }
        ],
        "cloud_info": {
          "placement_cloud": "local",
          "placement_region": "local",
          "placement_zone": "local"
        },
        "placement_uuid": ""
      },
      "role": "LEADER"
    },
    {
      "instance_id": {
        "permanent_uuid": "f32d67fbf54545b18d3aef17fee4032b",
        "instance_seqno": 1657972325360336,
        "start_time_us": 1657972325360336
      },
      "registration": {
        "private_rpc_addresses": [
          {
            "host": "yb-2.local",
            "port": 7100
          }
        ],
        "http_addresses": [
          {
            "host": "yb-2.local",
            "port": 7000
          }
        ],
        "cloud_info": {
          "placement_cloud": "local",
          "placement_region": "local",
          "placement_zone": "local"
        },
        "placement_uuid": ""
      },
      "role": "FOLLOWER"
    },
    {
      "instance_id": {
        "permanent_uuid": "b44e60f6a7f54aae98de54ee2e00736d",
        "instance_seqno": 1657972347087226,
        "start_time_us": 1657972347087226
      },
      "registration": {
        "private_rpc_addresses": [
          {
            "host": "yb-3.local",
            "port": 7100
          }
        ],
        "http_addresses": [
          {
            "host": "yb-3.local",
            "port": 7000
          }
        ],
        "cloud_info": {
          "placement_cloud": "local",
          "placement_region": "local",
          "placement_zone": "local"
        },
        "placement_uuid": ""
      },
      "role": "FOLLOWER"
    }
  ]
}
        "#.to_string();
        let result = AllStoredMasters::parse_masters(json, "", "");
        assert!(result.masters[0].error.is_none());
    }

    use crate::utility;

    #[test]
    fn integration_parse_masters() {
        let mut allstoredmasters = AllStoredMasters::new();
        let hostname = utility::get_hostname_master();
        let port = utility::get_port_master();

        let data_parsed_from_json = AllStoredMasters::read_http(hostname.as_ref(), port.as_ref());
        allstoredmasters.split_into_vectors(data_parsed_from_json, format!("{}:{}", &hostname, &port).as_ref(), Local::now());

        // a MASTER only will generate entities on each master (!)
        assert!(!allstoredmasters.stored_masters.is_empty());
        assert!(!allstoredmasters.stored_rpc_addresses.is_empty());
        assert!(!allstoredmasters.stored_http_addresses.is_empty());
    }
}