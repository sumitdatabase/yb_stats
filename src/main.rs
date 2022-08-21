use structopt::StructOpt;
use std::process;
use regex::Regex;
use chrono::Local;
use std::io::{stdin, Write};
use std::env;
use dotenv::dotenv;
use std::collections::HashMap;
use std::fs;
use log::*;

//mod yb_statsmetrics;
use yb_stats::metrics::{print_metrics_diff_for_snapshots, print_diff_metrics, get_metrics_into_diff_first_snapshot, get_metrics_into_diff_second_snapshot};


// structs from lib
use yb_stats::Snapshot;

// functions from lib
use yb_stats::{perform_snapshot,
               read_snapshots_from_file,
               read_begin_end_snapshot_from_user};

mod memtrackers;
use memtrackers::print_memtrackers_data;
mod loglines;
use loglines::print_loglines;
mod versions;
use versions::print_version_data;
mod threads;
use threads::print_threads_data;
mod gflags;
use gflags::print_gflags_data;
mod rpcs;
use rpcs::print_rpcs;
use yb_stats::node_exporter::{get_nodeexporter_into_diff_first_snapshot, get_nodeexpoter_into_diff_second_snapshot, print_diff_nodeexporter, print_nodeexporter_diff_for_snapshots};

mod statements;
use yb_stats::statements::{print_diff_statements, print_statements_diff_for_snapshots, get_statements_into_diff_first_snapshot, get_statements_into_diff_second_snapshot};

mod entities;
use entities::print_entities;
mod masters;
use masters::print_masters;

const DEFAULT_HOSTS: &str = "192.168.66.80,192.168.66.81,192.168.66.82";
const DEFAULT_PORTS: &str = "7000,9000,12000,13000,9300";
const DEFAULT_PARALLEL: &str = "1";
const WRITE_DOTENV: bool = true;

#[derive(Debug, StructOpt)]
struct Opts {
    /// hostnames (comma separated) default 192.168.66.80,192.168.66.81,192.168.66.82
    #[structopt(short, long, value_name = "hostname,hostname")]
    hosts: Option<String>,
    /// port numbers (comma separated) default 7000,9000,12000,13000,9300
    #[structopt(short, long, value_name = "port,port")]
    ports: Option<String>,
    /// regex to filter statistic names
    #[structopt(short, long, value_name = "regex")]
    stat_name_match: Option<String>,
    /// regex to filter table names (requires --details-enable)
    #[structopt(short, long, value_name = "regex")]
    table_name_match: Option<String>,
    /// regex to select hostnames or ports
    #[structopt(long, value_name = "regex")]
    hostname_match: Option<String>,
    /// add statistics that are not counters
    #[structopt(short, long)]
    gauges_enable: bool,
    /// report each table and tablet individually
    #[structopt(short, long)]
    details_enable: bool,
    /// be as silent as possible, only errors are printed
    #[structopt(long)]
    silent: bool,
    /// perform a CSV (stored) snapshot
    #[structopt(long)]
    snapshot: bool,
    /// comment to be added with the snapshot
    #[structopt(long, value_name = "\"comment\"")]
    snapshot_comment: Option<String>,
    /// this lists the snapshots, and allows you to select a begin and end snapshot for a diff report
    #[structopt(long)]
    snapshot_diff: bool,
    /// print memtrackers data for the given snapshot
    #[structopt(long, value_name = "snapshot number")]
    print_memtrackers: Option<String>,
    /// print log data for the given snapshot
    #[structopt(long, value_name = "snapshot number")]
    print_log: Option<String>,
    /// print entity data for the given snapshot
    #[structopt(long, value_name = "snapshot number")]
    print_entities: Option<String>,
    /// print master info for the given snapshot
    #[structopt(long, value_name = "snapshot number")]
    print_masters: Option<String>,
    /// print version data for the given snapshot
    #[structopt(long, value_name = "snapshot number")]
    print_version: Option<String>,
    /// print rpcs for the given snapshot
    #[structopt(long, value_name = "snapshot number")]
    print_rpcs: Option<String>,
    /// print threads data for the given snapshot
    #[structopt(long, value_name = "snapshot number")]
    print_threads: Option<String>,
    /// print gflags for the given snapshot
    #[structopt(long, value_name = "snapshot number")]
    print_gflags: Option<String>,
    /// log data severity to include: optional: I
    #[structopt(long, default_value = "WEF")]
    log_severity: String,
    /// how much threads to use in parallel for fetching data (default 1)
    #[structopt(long, value_name = "nr")]
    parallel: Option<String>,
    /// disable gathering of thread stacks from /threadz
    #[structopt(long)]
    disable_threads: bool,
    /// the length of the SQL text display
    #[structopt(long, value_name = "nr", default_value = "80")]
    sql_length: i32,
}

fn main() {
    env_logger::init();
    let mut changed_options = HashMap::new();
    dotenv().ok();
    let options = Opts::from_args();

    /*
     * Hosts
     * - if hosts is set, it's detected by is_some() and we take the set value, and set the changed_options HashMap for later write.
     * - if hosts is not set, we can detect if it's set via .env by looking at YBSTATS_HOSTS.
     *   - If YBSTATS_HOSTS is set, it's detected by Ok(), we set the changed_options HashMap for later write and return the set value.
     *   - if YBSTATS_HOSTS is not set, it will trigger Err(), and DEFAULT_HOSTS is used.
     */
    let hosts_string = if options.hosts.is_some() {
        info!("hosts argument set: using: {}", &options.hosts.as_ref().unwrap());
        changed_options.insert("YBSTATS_HOSTS", options.hosts.as_ref().unwrap().to_string());
        options.hosts.unwrap()
    } else {
        match env::var("YBSTATS_HOSTS") {
            Ok(set_var) => {
                info!("hosts not set: set via .env: YBSTATS_HOSTS: {}", set_var);
                changed_options.insert("YBSTATS_HOSTS", set_var.to_owned());
                set_var
            },
            Err(_e) => {
                info!("hosts not set: and not set via .env: using DEFAULT_HOSTS: {}", DEFAULT_HOSTS.to_string());
                DEFAULT_HOSTS.to_string()
            },
        }
    };
    let static_hosts: &'static str = Box::leak(hosts_string.into_boxed_str());
    let hosts: Vec<&'static str> = static_hosts.split(',').collect();

    /*
     * Ports
     * - if ports is set, it's detected by is_some() and we take the set value, and set the changed_options HashMap for later write.
     * - if ports is not set, then we can detect if it's set via .env by looking at YBSTATS_PORTS.
     *   - If YBSTATS_PORTS is set, it's detected by Ok(), we set the changed_options HashMap for later write and return the set value.
     *   - if YBSTATS_PORTS is not set, it will trigger Err(), and DEFAULT_PORTS is used.
     */
    let ports_string = if options.ports.is_some() {
        info!("ports argument set: using: {}", &options.ports.as_ref().unwrap());
        changed_options.insert("YBSTATS_PORTS", options.ports.as_ref().unwrap().to_string());
        options.ports.unwrap()
    } else {
        match env::var("YBSTATS_PORTS") {
            Ok(set_var) => {
                info!("ports not set: set via .env: YBSTATS_PORTS: {}", set_var);
                changed_options.insert("YBSTATS_PORTS", set_var.to_owned());
                set_var
            },
            Err(_e) => {
                info!("ports not set: and not set via .env: using DEFAULT_PORTS: {}", DEFAULT_PORTS.to_string());
                DEFAULT_PORTS.to_string()
            },
        }
    };
    let static_ports: &'static str = Box::leak(ports_string.into_boxed_str());
    let ports: Vec<&'static str> = static_ports.split(',').collect();

    /*
     * Parallel
     * - if parallel is set, it's detected by is_some() and we take the set value, and set the changed_options HashMap for later write.
     * - if parallel is not set, then we can detect if it's set via .env by looking at YBSTATS_PARALLEL.
     *   - If YBSTATS_PARALLEL is set, it's detected by Ok(), we set the changed_options HashMap for later write and return the set value.
     *   - if YBSTATS_PARALLEL is not set, it will trigger Err(), and DEFAULT_PARALLEL is used.
     */
    let parallel_string = if options.parallel.is_some() {
        info!("parallel argument set: using: {}", &options.parallel.as_ref().unwrap());
        changed_options.insert("YBSTATS_PARALLEL", options.parallel.as_ref().unwrap().to_string());
        options.parallel.unwrap()
    } else {
        match env::var("YBSTATS_PARALLEL") {
            Ok(set_var) => {
                info!("parallel not set: set via .env: YBSTATS_PARALLEL: {}", set_var);
                changed_options.insert("YBSTATS_PARALLEL", set_var.to_owned());
                set_var
            },
            Err(_e) => {
                info!("parallel not set: and not set via .env: using DEFAULT_PARALLEL: {}", DEFAULT_PARALLEL.to_string());
                DEFAULT_PARALLEL.to_string()
            },
        }
    };
    let parallel: usize = parallel_string.parse().unwrap();

    let snapshot: bool = options.snapshot as bool;
    let gauges_enable: bool = options.gauges_enable as bool;
    let details_enable: bool = options.details_enable as bool;
    let snapshot_diff: bool = options.snapshot_diff as bool;
    let disable_threads: bool = options.disable_threads as bool;
    let silent: bool = options.silent as bool;
    let sql_length: usize = options.sql_length as usize;
    let log_severity: String = options.log_severity;
    let snapshot_comment = match options.snapshot_comment {
        Some(comment) => comment,
        None => String::from("")
    };
    let stat_name_filter = match options.stat_name_match {
        Some(stat_name_match) => Regex::new( stat_name_match.as_str() ).unwrap(),
        None => Regex::new( ".*" ).unwrap()
    };
    let hostname_filter = match options.hostname_match {
        Some(hostname_match) => Regex::new( hostname_match.as_str() ).unwrap(),
        None => Regex::new( ".*" ).unwrap()
    };
    let table_name_filter = match options.table_name_match {
        Some(table_name_match) => Regex::new( table_name_match.as_str() ).unwrap(),
        None => Regex::new( ".*" ).unwrap()
    };

    let current_directory = env::current_dir().unwrap();
    let yb_stats_directory = current_directory.join("yb_stats.snapshots");

    if snapshot {

        info!("snapshot option");
        let snapshot_number: i32 = perform_snapshot(hosts, ports, snapshot_comment, parallel, disable_threads);
        if ! silent {
            println!("snapshot number {}", snapshot_number);
        }

    } else if snapshot_diff {

        info!("snapshot_diff");
        let current_directory = env::current_dir().unwrap();
        let yb_stats_directory = current_directory.join("yb_stats.snapshots");
        let snapshots: Vec<Snapshot> = read_snapshots_from_file(&yb_stats_directory);

        for row in &snapshots {
            println!("{:>3} {:30} {:50}", row.number, row.timestamp, row.comment);
        }

        let (begin_snapshot, end_snapshot, begin_snapshot_row) = read_begin_end_snapshot_from_user(&snapshots);

        print_metrics_diff_for_snapshots(&begin_snapshot, &end_snapshot, &begin_snapshot_row.timestamp, &hostname_filter, &stat_name_filter, &table_name_filter, &details_enable, &gauges_enable);
        print_statements_diff_for_snapshots(&begin_snapshot, &end_snapshot, &begin_snapshot_row.timestamp, &hostname_filter, sql_length);
        print_nodeexporter_diff_for_snapshots(&begin_snapshot, &end_snapshot, &begin_snapshot_row.timestamp, &hostname_filter, &stat_name_filter, &gauges_enable, &details_enable);

    } else if options.print_memtrackers.is_some() {

        print_memtrackers_data(&options.print_memtrackers.unwrap(), &yb_stats_directory, &hostname_filter, &stat_name_filter);

    } else if options.print_log.is_some() {

        print_loglines(&options.print_log.unwrap(), &yb_stats_directory, &hostname_filter, &log_severity);

    } else if options.print_version.is_some() {

        print_version_data(&options.print_version.unwrap(), &yb_stats_directory, &hostname_filter);

    } else if options.print_threads.is_some() {

        print_threads_data(&options.print_threads.unwrap(), &yb_stats_directory, &hostname_filter);

    } else if options.print_gflags.is_some() {

        print_gflags_data(&options.print_gflags.unwrap(), &yb_stats_directory, &hostname_filter, &stat_name_filter);

    } else if options.print_entities.is_some() {

        print_entities(&options.print_entities.unwrap(), &yb_stats_directory, &hostname_filter, &table_name_filter);

    } else if options.print_masters.is_some() {

        print_masters(&options.print_masters.unwrap(), &yb_stats_directory, &hostname_filter);

    } else if options.print_rpcs.is_some() {

        print_rpcs(&options.print_rpcs.unwrap(), &yb_stats_directory, &hostname_filter, &details_enable);

    } else {

        info!("ad-hoc mode");
        let first_snapshot_time = Local::now();
        let (mut values_diff, mut countsum_diff, mut countsumrows_diff) = get_metrics_into_diff_first_snapshot(&hosts, &ports, parallel);
        let mut statements_diff = get_statements_into_diff_first_snapshot(&hosts, &ports, parallel);
        let mut node_exporter_diff = get_nodeexporter_into_diff_first_snapshot(&hosts, &ports, parallel);

        println!("Begin metrics snapshot created, press enter to create end snapshot for difference calculation.");
        let mut input = String::new();
        stdin().read_line(&mut input).expect("failed");

        let second_snapshot_time = Local::now();
        get_metrics_into_diff_second_snapshot(&hosts, &ports, &mut values_diff, &mut countsum_diff, &mut countsumrows_diff, &first_snapshot_time, parallel);
        get_statements_into_diff_second_snapshot(&hosts, &ports, &mut statements_diff, &first_snapshot_time, parallel);
        get_nodeexpoter_into_diff_second_snapshot(&hosts, &ports, &mut node_exporter_diff, &first_snapshot_time, parallel);

        println!("Time between snapshots: {:8.3} seconds", (second_snapshot_time-first_snapshot_time).num_milliseconds() as f64/1000_f64);
        print_diff_metrics(&values_diff, &countsum_diff, &countsumrows_diff, &hostname_filter, &stat_name_filter, &table_name_filter, &details_enable, &gauges_enable);
        print_diff_statements(&statements_diff, &hostname_filter, sql_length);
        print_diff_nodeexporter(&node_exporter_diff, &hostname_filter, &stat_name_filter, &gauges_enable, &details_enable);

    }

    if !changed_options.is_empty() && WRITE_DOTENV {
            info!("Writing .env file");
            let mut file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(".env")
            .unwrap_or_else(| e | {
                            error!("error writing .env file into current working directory: {}", e);
                            process::exit(1);
                            });
            for (key, value) in changed_options {
                file.write_all(format!("{}={}\n", key, value).as_bytes()).unwrap();
                info!("{}={}", key, value);
            }
            file.flush().unwrap();
    }
}
