use chrono::Local;
use std::env;

fn get_hostname_tserver() -> String {
    let hostname = match env::var("HOSTNAME_TSERVER") {
        Ok(value) => value,
        Err(e) => panic!("Error reading environment variable HOSTNAME_TSERVER: {:?}", e)
    };
    hostname
}
fn get_port_tserver() -> String {
    let port = match env::var("PORT_TSERVER") {
        Ok(value) => value,
        Err(e) => panic!("Error reading environment variable PORT_TSERVER: {:?}", e)
    };
    port
}
fn get_hostname_ysql() -> String {
    let hostname= match env::var("HOSTNAME_YSQL") {
        Ok(value) => value,
        Err(e) => panic!("Error reading environment variable HOSTNAME_YSQL: {:?}", e)
    };
    hostname
}
fn get_port_ysql() -> String {
    let port= match env::var("PORT_YSQL") {
        Ok(value) => value,
        Err(e) => panic!("Error reading environment variable PORT_YSQL: {:?}", e)
    };
    port
}
fn get_hostname_ycql() -> String {
    let hostname= match env::var("HOSTNAME_YCQL") {
        Ok(value) => value,
        Err(e) => panic!("Error reading environment variable HOSTNAME_YCQL: {:?}", e)
    };
    hostname
}
fn get_port_ycql() -> String {
    let port= match env::var("PORT_YCQL") {
        Ok(value) => value,
        Err(e) => panic!("Error reading environment variable PORT_YCQL: {:?}", e)
    };
    port
}
fn get_hostname_yedis() -> String {
    let hostname= match env::var("HOSTNAME_YEDIS") {
        Ok(value) => value,
        Err(e) => panic!("Error reading environment variable HOSTNAME_YEDIS: {:?}", e)
    };
    hostname
}
fn get_port_yedis() -> String {
    let port= match env::var("HOSTNAME_PORT") {
        Ok(value) => value,
        Err(e) => panic!("Error reading environment variable PORT_YEDIS: {:?}", e)
    };
    port
}
fn get_hostname_master() -> String {
    let hostname= match env::var("HOSTNAME_MASTER") {
        Ok(value) => value,
        Err(e) => panic!("Error reading environment variable HOSTNAME_MASTER: {:?}", e)
    };
    hostname
}
fn get_port_master() -> String {
    let port= match env::var("PORT_MASTER") {
        Ok(value) => value,
        Err(e) => panic!("Error reading environment variable PORT_MASTER: {:?}", e)
    };
    port
}

use yb_stats::gflags::{StoredGFlags, read_gflags, add_to_gflags_vector};
#[test]
fn parse_gflags_master() {
    let mut stored_gflags: Vec<StoredGFlags> = Vec::new();
    let detail_snapshot_time = Local::now();
    let hostname= get_hostname_master();
    let port = get_port_master();

    let gflags = read_gflags(&hostname.as_str(), &port.as_str());
    add_to_gflags_vector(gflags, format!("{}:{}", hostname, port).as_str(), detail_snapshot_time, &mut stored_gflags);
    // the master must have gflags
    assert!(stored_gflags.len() > 0);
}
#[test]
fn parse_gflags_tserver() {
    let mut stored_gflags: Vec<StoredGFlags> = Vec::new();
    let detail_snapshot_time = Local::now();
    let hostname = get_hostname_tserver();
    let port = get_port_tserver();

    let gflags = read_gflags(&hostname.as_str(), &port.as_str());
    add_to_gflags_vector(gflags, format!("{}:{}", hostname, port).as_str(), detail_snapshot_time, &mut stored_gflags);
    // the tserver must have gflags
    assert!(stored_gflags.len() > 0);
}

use yb_stats::memtrackers::{MemTrackers, StoredMemTrackers, read_memtrackers, add_to_memtrackers_vector};
#[test]
fn parse_memtrackers_master() {
    let mut stored_memtrackers: Vec<StoredMemTrackers> = Vec::new();
    let detail_snapshot_time = Local::now();
    let hostname = get_hostname_master();
    let port = get_port_master();

    let memtrackers: Vec<MemTrackers> = read_memtrackers(&hostname.as_str(), &port.as_str());
    add_to_memtrackers_vector(memtrackers, format!("{}:{}", hostname, port).as_str(), detail_snapshot_time, &mut stored_memtrackers);
    // memtrackers must return some rows
    assert!(stored_memtrackers.len() > 0);
}
#[test]
fn parse_memtrackers_tserver() {
    let mut stored_memtrackers: Vec<StoredMemTrackers> = Vec::new();
    let detail_snapshot_time = Local::now();
    let hostname= get_hostname_tserver();
    let port = get_port_tserver();

    let memtrackers: Vec<MemTrackers> = read_memtrackers(&hostname.as_str(), &port.as_str());
    add_to_memtrackers_vector(memtrackers, format!("{}:{}", hostname, port).as_str(), detail_snapshot_time, &mut stored_memtrackers);
    // memtrackers must return some rows
    assert!(stored_memtrackers.len() > 0);
}

use yb_stats::loglines::{StoredLogLines, read_loglines, add_to_loglines_vector};
#[test]
fn parse_loglines_master() {
    let mut stored_loglines: Vec<StoredLogLines> = Vec::new();
    let hostname= get_hostname_master();
    let port = get_port_master();

    let loglines = read_loglines(&hostname.as_str(), &port.as_str());
    add_to_loglines_vector(loglines, format!("{}:{}", hostname, port).as_str(), &mut stored_loglines);
    // it's likely there will be logging
    assert!(stored_loglines.len() > 0);
}
#[test]
fn parse_loglines_tserver() {
    let mut stored_loglines: Vec<StoredLogLines> = Vec::new();
    let hostname= get_hostname_tserver();
    let port = get_port_tserver();

    let loglines = read_loglines(&hostname.as_str(), &port.as_str());
    add_to_loglines_vector(loglines, format!("{}:{}", hostname, port).as_str(), &mut stored_loglines);
    // it's likely there will be logging
    assert!(stored_loglines.len() > 0);
}

use yb_stats::versions::{StoredVersionData, read_version, add_to_version_vector};
#[test]
fn parse_versiondata_master() {
    let mut stored_versiondata: Vec<StoredVersionData> = Vec::new();
    let detail_snapshot_time = Local::now();
    let hostname = get_hostname_master();
    let port = get_port_master();

    let data_parsed_from_json = read_version(&hostname.as_str(), &port.as_str());
    add_to_version_vector(data_parsed_from_json, format!("{}:{}", hostname, port).as_str(), detail_snapshot_time, &mut stored_versiondata);
    // each daemon should return one row.
    assert!(stored_versiondata.len() == 1);
}
#[test]
fn parse_versiondata_tserver() {
    let mut stored_versiondata: Vec<StoredVersionData> = Vec::new();
    let detail_snapshot_time = Local::now();
    let hostname = get_hostname_tserver();
    let port = get_port_tserver();

    let data_parsed_from_json = read_version(&hostname.as_str(), &port.as_str());
    add_to_version_vector(data_parsed_from_json, format!("{}:{}", hostname, port).as_str(), detail_snapshot_time, &mut stored_versiondata);
    // each daemon should return one row.
    assert!(stored_versiondata.len() == 1);
}

use yb_stats::threads::{StoredThreads, read_threads, add_to_threads_vector};
#[test]
fn parse_threadsdata_master() {
    let mut stored_threadsdata: Vec<StoredThreads> = Vec::new();
    let detail_snapshot_time = Local::now();
    let hostname = get_hostname_master();
    let port = get_port_master();

    let data_parsed_from_json = read_threads(&hostname.as_str(), &port.as_str());
    add_to_threads_vector(data_parsed_from_json, format!("{}:{}", hostname, port).as_str(), detail_snapshot_time, &mut stored_threadsdata);
    // each daemon should return one row.
    assert!(stored_threadsdata.len() > 1);
}
#[test]
fn parse_threadsdata_tserver() {
    let mut stored_threadsdata: Vec<StoredThreads> = Vec::new();
    let detail_snapshot_time = Local::now();
    let hostname = get_hostname_tserver();
    let port = get_port_tserver();

    let data_parsed_from_json = read_threads(&hostname.as_str(), &port.as_str());
    add_to_threads_vector(data_parsed_from_json, format!("{}:{}", hostname, port).as_str(), detail_snapshot_time, &mut stored_threadsdata);
    // each daemon should return one row.
    assert!(stored_threadsdata.len() > 1);
}

use yb_stats::statements::{StoredStatements, read_statements, add_to_statements_vector};
#[test]
fn parse_statements_ysql() {
    let mut stored_statements: Vec<StoredStatements> = Vec::new();
    let detail_snapshot_time = Local::now();
    let hostname = get_hostname_ysql();
    let port = get_port_ysql();

    let data_parsed_from_json = read_statements(&hostname.as_str(), &port.as_str());
    add_to_statements_vector(data_parsed_from_json, format!("{}:{}", hostname, port).as_str(), detail_snapshot_time, &mut stored_statements);
    // likely in a test scenario, there are no SQL commands executed, and thus no rows are returned.
    // to make sure this test works in both the scenario of no statements, and with statements, perform no assertion.
}

use yb_stats::metrics::{StoredValues,StoredCountSum, StoredCountSumRows, read_metrics, add_to_metric_vectors};
#[test]
fn parse_metrics_master() {
    let mut stored_values: Vec<StoredValues> = Vec::new();
    let mut stored_countsum: Vec<StoredCountSum> = Vec::new();
    let mut stored_countsumrows: Vec<StoredCountSumRows> = Vec::new();
    let detail_snapshot_time = Local::now();
    let hostname = get_hostname_master();
    let port = get_port_master();

    let data_parsed_from_json = read_metrics(&hostname.as_str(), &port.as_str());
    add_to_metric_vectors(data_parsed_from_json, format!("{}:{}", hostname, port).as_str(), detail_snapshot_time, &mut stored_values, &mut stored_countsum, &mut stored_countsumrows);
    // a master will produce values and countsum rows, but no countsumrows rows, because that belongs to YSQL.
    assert!(stored_values.len() > 0);
    assert!(stored_countsum.len() > 0);
    assert!(stored_countsumrows.len() == 0);
}
#[test]
fn parse_metrics_tserver() {
    let mut stored_values: Vec<StoredValues> = Vec::new();
    let mut stored_countsum: Vec<StoredCountSum> = Vec::new();
    let mut stored_countsumrows: Vec<StoredCountSumRows> = Vec::new();
    let detail_snapshot_time = Local::now();
    let hostname = get_hostname_tserver();
    let port = get_port_tserver();

    let data_parsed_from_json = read_metrics(&hostname.as_str(), &port.as_str());
    add_to_metric_vectors(data_parsed_from_json, format!("{}:{}", hostname, port).as_str(), detail_snapshot_time, &mut stored_values, &mut stored_countsum, &mut stored_countsumrows);
    // a master will produce values and countsum rows, but no countsumrows rows, because that belongs to YSQL.
    assert!(stored_values.len() > 0);
    assert!(stored_countsum.len() > 0);
    assert!(stored_countsumrows.len() == 0);
}
#[test]
fn parse_metrics_ysql() {
    let mut stored_values: Vec<StoredValues> = Vec::new();
    let mut stored_countsum: Vec<StoredCountSum> = Vec::new();
    let mut stored_countsumrows: Vec<StoredCountSumRows> = Vec::new();
    let detail_snapshot_time = Local::now();
    let hostname = get_hostname_ysql();
    let port = get_port_ysql();

    let data_parsed_from_json = read_metrics(&hostname.as_str(), &port.as_str());
    add_to_metric_vectors(data_parsed_from_json, format!("{}:{}", hostname, port).as_str(), detail_snapshot_time, &mut stored_values, &mut stored_countsum, &mut stored_countsumrows);
    // YSQL will produce countsumrows rows, but no value or countsum rows
    assert!(stored_values.len() == 0);
    assert!(stored_countsum.len() == 0);
    assert!(stored_countsumrows.len() > 0);
}
#[test]
fn parse_metrics_ycql() {
    let mut stored_values: Vec<StoredValues> = Vec::new();
    let mut stored_countsum: Vec<StoredCountSum> = Vec::new();
    let mut stored_countsumrows: Vec<StoredCountSumRows> = Vec::new();
    let detail_snapshot_time = Local::now();
    let hostname = get_hostname_ycql();
    let port = get_port_ycql();

    let data_parsed_from_json = read_metrics(&hostname.as_str(), &port.as_str());
    add_to_metric_vectors(data_parsed_from_json, format!("{}:{}", hostname, port).as_str(), detail_snapshot_time, &mut stored_values, &mut stored_countsum, &mut stored_countsumrows);
    // YCQL will produce values and countsum rows, but no countsumrows rows, because that belongs to YSQL.
    // countsum rows are filtered on count == 0, which is true if it wasn't used. therefore, we do not check on countsum statistics. likely, YCQL wasn't used prior to the test.
    assert!(stored_values.len() > 0);
    //assert!(stored_countsum.len() > 0);
    assert!(stored_countsumrows.len() == 0);
}
#[test]
fn parse_metrics_yedis() {
    let mut stored_values: Vec<StoredValues> = Vec::new();
    let mut stored_countsum: Vec<StoredCountSum> = Vec::new();
    let mut stored_countsumrows: Vec<StoredCountSumRows> = Vec::new();
    let detail_snapshot_time = Local::now();
    let hostname = get_hostname_yedis();
    let port = get_port_yedis();

    let data_parsed_from_json = read_metrics(&hostname.as_str(), &port.as_str());
    add_to_metric_vectors(data_parsed_from_json, format!("{}:{}", hostname, port).as_str(), detail_snapshot_time, &mut stored_values, &mut stored_countsum, &mut stored_countsumrows);
    // YEDIS will produce values and countsum rows, but no countsumrows rows, because that belongs to YSQL.
    // countsum rows are filtered on count == 0, which is true when it wasn't used. therefore, we do not check on countsum statistics. likely, YEDIS wasn't used prior to the test.
    assert!(stored_values.len() > 0);
    //assert!(stored_countsum.len() > 0);
    assert!(stored_countsumrows.len() == 0);
}