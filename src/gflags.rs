use std::path::PathBuf;
use std::time::Instant;
use chrono::{DateTime, Local};
use port_scanner::scan_port_addr;
use regex::Regex;
use std::fs;
use std::process;
use serde_derive::{Serialize,Deserialize};
use std::sync::mpsc::channel;
use log::*;

#[derive(Debug)]
pub struct GFlag {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StoredGFlags {
    pub hostname_port: String,
    pub timestamp: DateTime<Local>,
    pub gflag_name: String,
    pub gflag_value: String,
}

#[allow(dead_code)]
pub fn read_gflags(
    host: &str,
    port: &str,
) -> Vec<GFlag> {
    if ! scan_port_addr( format!("{}:{}", host, port)) {
        warn!("hostname:port {}:{} cannot be reached, skipping (gflags)",host ,port);
        return Vec::new();
    }
    /*
    if let Ok(data_from_http) = reqwest::blocking::get(format!("http://{}:{}/varz?raw",host ,port)) {
        parse_gflags(data_from_http.text().unwrap())
    } else {
        parse_gflags(String::from(""))
    }

     */
    let data_from_http = reqwest::blocking::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap()
        .get(format!("http://{}:{}/varz?raw", host, port))
        .send()
        .unwrap()
        .text()
        .unwrap();
    parse_gflags(data_from_http)
}

#[allow(dead_code)]
#[allow(clippy::ptr_arg)]
pub async fn perform_gflags_snapshot(
    hosts: &Vec<&str>,
    ports: &Vec<&str>,
    snapshot_number: i32,
    yb_stats_directory: &PathBuf,
    parallel: usize
) {
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
                    let gflags = read_gflags(host, port);
                    tx.send((format!("{}:{}", host, port), detail_snapshot_time, gflags)).expect("error sending data via tx (gflags)");
                });
            }}
    });
    info!("end parallel http read {:?}", timer.elapsed());

    let mut stored_gflags: Vec<StoredGFlags> = Vec::new();
    for (hostname_port, detail_snapshot_time, gflags) in rx {
        add_to_gflags_vector(gflags, &hostname_port, detail_snapshot_time, &mut stored_gflags);
    }

    let current_snapshot_directory = &yb_stats_directory.join(&snapshot_number.to_string());
    let gflags_file = &current_snapshot_directory.join("gflags");
    let file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(gflags_file)
        .unwrap_or_else(|e| {
            error!("Fatal: error writing gflags data in snapshot directory {}: {}", &gflags_file.clone().into_os_string().into_string().unwrap(), e);
            process::exit(1);
        });
    let mut writer = csv::Writer::from_writer(file);
    for row in stored_gflags {
        writer.serialize(row).unwrap();
    }
    writer.flush().unwrap();
}

#[allow(dead_code)]
pub fn add_to_gflags_vector(gflagdata: Vec<GFlag>,
                            hostname: &str,
                            snapshot_time: DateTime<Local>,
                            stored_gflags: &mut Vec<StoredGFlags>
) {
    for gflag in gflagdata {
        stored_gflags.push( StoredGFlags {
            hostname_port: hostname.to_string(),
            timestamp: snapshot_time,
            gflag_name: gflag.name.to_string(),
            gflag_value: gflag.value.to_string()
        });
    }
}

#[allow(dead_code)]
fn parse_gflags( gflags_data: String ) -> Vec<GFlag> {
    let mut gflags: Vec<GFlag> = Vec::new();
    let re = Regex::new( r"--([A-Za-z_0-9]*)=(.*)\n" ).unwrap();
    for captures in re.captures_iter(&gflags_data) {
        gflags.push(GFlag { name: captures.get(1).unwrap().as_str().to_string(), value: captures.get(2).unwrap().as_str().to_string() });
    }
    gflags
}

#[allow(clippy::ptr_arg)]
fn read_gflags_snapshot(snapshot_number: &String, yb_stats_directory: &PathBuf) -> Vec<StoredGFlags> {
    let mut stored_gflags: Vec<StoredGFlags> = Vec::new();
    let gflags_file = &yb_stats_directory.join(snapshot_number).join("gflags");
    let file = fs::File::open(gflags_file)
        .unwrap_or_else(|e| {
            error!("Fatal: error reading file: {}: {}", &gflags_file.clone().into_os_string().into_string().unwrap(), e);
            process::exit(1);
        });
    let mut reader = csv::Reader::from_reader(file);
    for row in reader.deserialize() {
        let data: StoredGFlags = row.unwrap();
        let _ = &stored_gflags.push(data);
    }
    stored_gflags
}

pub fn print_gflags_data(
    snapshot_number: &String,
    yb_stats_directory: &PathBuf,
    hostname_filter: &Regex,
    stat_name_filter: &Regex,
) {
    info!("print_gflags");
    let stored_gflags: Vec<StoredGFlags> = read_gflags_snapshot(snapshot_number, yb_stats_directory);
    let mut previous_hostname_port = String::from("");
    for row in stored_gflags {
        if hostname_filter.is_match(&row.hostname_port) &&
            stat_name_filter.is_match( &row.gflag_name) {
            if row.hostname_port != previous_hostname_port {
                println!("--------------------------------------------------------------------------------------------------------------------------------------");
                println!("Host: {}, Snapshot number: {}, Snapshot time: {}", &row.hostname_port.to_string(), &snapshot_number, row.timestamp);
                println!("--------------------------------------------------------------------------------------------------------------------------------------");
                previous_hostname_port = row.hostname_port.to_string();
            }
            println!("{:80} {:30}", row.gflag_name, row.gflag_value)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_parse_regular_gflags() {
        // These are the gflags of the tserver of version 2.15.2.1
        let gflags = r#"
Command-line Flags--TEST_xcluster_simulate_have_more_records=false
--cdc_enable_replicate_intents=true
--cdc_transaction_timeout_ms=0
--TEST_block_get_changes=false
--TEST_cdc_log_init_failure_timeout_seconds=0
--cdc_get_changes_free_rpc_ratio=0.10000000000000001
--cdc_read_rpc_timeout_ms=30000
--cdc_read_safe_deadline_ratio=0.10000000000000001
--cdc_state_checkpoint_update_interval_ms=15000
--cdc_write_rpc_timeout_ms=30000
--cdc_ybclient_reactor_threads=50
--certs_for_cdc_dir=
--enable_cdc_client_tablet_caching=false
--enable_cdc_state_table_caching=true
--enable_collect_cdc_metrics=true
--enable_update_local_peer_min_index=false
--parallelize_bootstrap_producer=true
--update_metrics_interval_ms=15000
--update_min_cdc_indices_interval_secs=60
--wait_replication_drain_tserver_max_retry=3
--wait_replication_drain_tserver_retry_interval_ms=100
--cdc_snapshot_batch_size=250
--stream_truncate_record=false
--cert_file_pattern=node.$0.crt
--cert_node_filename=
--certs_dir=
--certs_for_client_dir=
--enable_stream_compression=true
--key_file_pattern=node.$0.key
--node_to_node_encryption_required_uid=
--node_to_node_encryption_use_client_certificates=false
--use_client_to_server_encryption=false
--use_node_to_node_encryption=false
--TEST_tablet_delay_restore_ms=0
--cdc_consumer_handler_thread_pool_size=0
--async_replication_idle_delay_ms=100
--async_replication_max_idle_wait=3
--async_replication_polling_delay_ms=0
--cdc_consumer_use_proxy_forwarding=false
--replication_failure_delay_exponent=16
--ts_backup_svc_num_threads=4
--ts_backup_svc_queue_length=50
--xcluster_svc_queue_length=5000
--TEST_xcluster_consumer_fail_after_process_split_op=false
--cdc_force_remote_tserver=false
--TEST_twodc_write_hybrid_time=false
--cdc_max_apply_batch_num_records=1024
--cdc_max_apply_batch_size_bytes=0
--TEST_force_master_leader_resolution=false
--detect_duplicates_for_retryable_requests=true
--forward_redis_requests=true
--ybclient_print_trace_every_n=0
--ysql_forward_rpcs_to_local_tserver=false
--TEST_auto_flags_initialized=true
--auto_flags_load_from_master_backoff_increment_ms=100
--auto_flags_load_from_master_max_backoff_sec=3
--disable_auto_flags_management=false
--TEST_combine_batcher_errors=false
--TEST_simulate_tablet_lookup_does_not_match_partition_key_probability=0
--TEST_assert_local_tablet_server_selected=false
--TEST_assert_tablet_server_select_is_in_zone=
--backfill_index_client_rpc_timeout_ms=86400000
--client_suppress_created_logs=false
--ycql_num_tablets=-1
--ysql_num_tablets=-1
--yb_client_admin_operation_timeout_sec=120
--yb_client_num_reactors=16
--reset_master_leader_timeout_ms=15000
--TEST_force_master_lookup_all_tablets=false
--TEST_simulate_lookup_partition_list_mismatch_probability=0
--TEST_simulate_lookup_timeout_probability=0
--TEST_verify_all_replicas_alive=false
--max_concurrent_master_lookups=500
--meta_cache_lookup_throttling_max_delay_ms=1000
--meta_cache_lookup_throttling_step_ms=5
--retry_failed_replica_ms=60000
--update_permissions_cache_msecs=2000
--client_read_write_timeout_ms=600000
--max_num_tablets_for_table=5000
--TEST_assert_failed_replicas_less_than=0
--TEST_assert_local_op=false
--force_lookup_cache_refresh_secs=0
--lookup_cache_refresh_secs=60
--update_all_tablets_upon_network_failure=true
--TEST_disable_proactive_txn_cleanup_on_abort=false
--TEST_old_txn_status_abort_delay_ms=0
--TEST_transaction_inject_flushed_delay_ms=0
--TEST_txn_status_moved_rpc_send_delay_ms=0
--auto_promote_nonlocal_transactions_to_global=true
--transaction_disable_heartbeat_in_tests=false
--transaction_heartbeat_usec=500000
--txn_print_trace_every_n=0
--txn_slow_op_threshold_ms=0
--transaction_manager_queue_limit=500
--transaction_manager_workers_limit=50
--TEST_track_last_transaction=false
--force_global_transactions=false
--transaction_pool_cleanup_interval_ms=5000
--transaction_pool_reserve_factor=2
--redis_allow_reads_from_followers=false
--yb_system_namespace_readonly=true
--wait_hybrid_time_sleep_interval_us=10000
--TEST_enable_db_catalog_version_mode=false
--disable_hybrid_scan=false
--enable_automatic_tablet_splitting=true
--enable_pg_savepoints=true
--log_ysql_catalog_versions=false
--yb_num_shards_per_tserver=2
--ysql_disable_index_backfill=false
--ysql_num_shards_per_tserver=1
--yql_max_value_size=67108864
--TEST_partitioning_version=-1
--transaction_rpc_timeout_ms=5000
--use_private_ip=never
--TEST_process_info_dir=
--TEST_fault_crash_before_cmeta_flush=0
--TEST_delay_removing_peer_with_failed_tablet_secs=0
--TEST_enable_remote_bootstrap=true
--TEST_fault_crash_on_leader_request_fraction=0
--consensus_rpc_timeout_ms=3000
--max_wait_for_processresponse_before_closing_ms=5000
--TEST_disallow_lmp_failures=false
--cdc_checkpoint_opid_interval_ms=60000
--cdc_intent_retention_ms=14400000
--consensus_inject_latency_ms_in_notifications=0
--consensus_lagging_follower_threshold=10
--consensus_max_batch_size_bytes=4194304
--enable_consensus_exponential_backoff=true
--follower_unavailable_considered_failed_sec=900
--TEST_log_consider_all_ops_safe=false
--TEST_log_fault_after_segment_allocation_min_replicate_index=0
--TEST_simulate_abrupt_server_restart=false
--consensus_log_scoped_watch_delay_append_threshold_ms=1000
--consensus_log_scoped_watch_delay_callback_threshold_ms=1000
--log_inject_append_latency_ms_max=0
--log_inject_latency=false
--log_inject_latency_ms_mean=100
--log_inject_latency_ms_stddev=100
--log_min_seconds_to_retain=900
--log_min_segments_to_retain=2
--taskstream_queue_max_size=100000
--taskstream_queue_max_wait_ms=1000
--time_based_wal_gc_clock_delta_usec=0
--wait_for_safe_op_id_to_apply_default_timeout_ms=15000
--TEST_log_cache_skip_eviction=false
--global_log_cache_size_limit_mb=32
--global_log_cache_size_limit_percentage=5
--log_cache_size_limit_mb=16
--TEST_entries_per_log_index_chuck=0
--entries_per_index_block=10000
--TEST_get_changes_read_loop_delay_ms=0
--TEST_record_segments_violate_max_time_policy=false
--TEST_record_segments_violate_min_space_policy=false
--enable_log_retention_by_op_idx=true
--get_changes_honor_deadline=true
--log_max_seconds_to_retain=86400
--log_stop_retaining_min_disk_mb=102400
--bytes_durable_wal_write_mb=1
--durable_wal_write=false
--initial_log_segment_size_bytes=1048576
--interval_durable_wal_write_ms=1000
--log_async_preallocate_segments=true
--log_preallocate_segments=true
--log_segment_size_bytes=0
--log_segment_size_mb=64
--require_durable_wal_write=false
--save_index_into_wal_segments=false
--enable_multi_raft_heartbeat_batcher=false
--multi_raft_batch_size=0
--multi_raft_heartbeat_interval_ms=50
--TEST_do_not_start_election_test_only=false
--TEST_follower_fail_all_prepare=false
--TEST_follower_pause_update_consensus_requests=false
--TEST_follower_reject_update_consensus_requests=false
--TEST_follower_reject_update_consensus_requests_seconds=0
--TEST_inject_delay_leader_change_role_append_secs=0
--TEST_log_change_config_every_n=1
--TEST_pause_update_majority_replicated=false
--TEST_pause_update_replica=false
--TEST_return_error_on_change_config=0
--after_stepdown_delay_election_multiplier=5
--enable_leader_failure_detection=true
--enable_lease_revocation=true
--evict_failed_followers=true
--ht_lease_duration_ms=2000
--leader_failure_exp_backoff_max_delta_ms=20000
--leader_failure_max_missed_heartbeat_periods=6
--leader_lease_duration_ms=4000
--min_leader_stepdown_retry_interval_ms=20000
--protege_synchronization_timeout_ms=1000
--quick_leader_election_on_create=false
--raft_disallow_concurrent_outstanding_report_failure_tasks=true
--raft_heartbeat_interval_ms=1000
--stepdown_disable_graceful_transition=false
--temporary_disable_preelections_timeout_ms=600000
--use_preelection=true
--inject_delay_commit_pre_voter_to_voter_secs=0
--retryable_request_range_time_limit_secs=30
--retryable_request_timeout_secs=660
--file_expiration_ignore_value_ttl=false
--file_expiration_value_ttl_overrides_table_ttl=false
--TEST_pause_write_apply_after_if=false
--ycql_consistent_transactional_paging=false
--ycql_disable_index_updating_optimization=false
--ycql_enable_packed_row=false
--ycql_packed_row_size_limit=0
--TEST_tserver_timeout=false
--cdc_max_stream_intent_records=1000
--aborted_intent_cleanup_max_batch_size=256
--aborted_intent_cleanup_ms=60000
--external_intent_cleanup_secs=86400
--intents_compaction_filter_max_errors_to_log=100
--block_restart_interval=16
--compression_type=Snappy
--db_block_size_bytes=32768
--db_filter_block_size_bytes=65536
--db_index_block_size_bytes=32768
--db_min_keys_per_index_block=100
--db_write_buffer_size=-1
--enable_ondisk_compression=true
--index_block_restart_interval=1
--initial_seqno=1125899906842624
--max_nexts_to_avoid_seek=2
--memstore_size_mb=128
--num_reserved_small_compaction_threads=-1
--prioritize_tasks_by_disk=false
--priority_thread_pool_size=-1
--regular_tablets_data_block_key_value_encoding=shared_prefix
--rocksdb_base_background_compactions=-1
--rocksdb_compact_flush_rate_limit_bytes_per_sec=1073741824
--rocksdb_compact_flush_rate_limit_sharing_mode=tserver
--rocksdb_compaction_measure_io_stats=false
--rocksdb_compaction_size_threshold_bytes=2147483648
--rocksdb_disable_compactions=false
--rocksdb_level0_file_num_compaction_trigger=5
--rocksdb_level0_slowdown_writes_trigger=-1
--rocksdb_level0_stop_writes_trigger=-1
--rocksdb_max_background_compactions=-1
--rocksdb_max_background_flushes=-1
--rocksdb_max_file_size_for_compaction=0
--rocksdb_max_write_buffer_number=2
--rocksdb_universal_compaction_always_include_size_threshold=67108864
--rocksdb_universal_compaction_min_merge_width=4
--rocksdb_universal_compaction_size_ratio=20
--use_docdb_aware_bloom_filter=true
--use_multi_level_index=true
--dump_lock_keys=true
--TEST_slowdown_pgsql_aggregate_read_ms=0
--TEST_ysql_suppress_ybctid_corruption_details=false
--pgsql_consistent_transactional_paging=true
--ysql_enable_packed_row=false
--ysql_packed_row_size_limit=0
--ysql_scan_deadline_margin_ms=1000
--ysql_scan_timeout_multiplier=0.5
--emulate_redis_responses=true
--TEST_docdb_sort_weak_intents=false
--TEST_fail_on_replicated_batch_idx_set_in_txn_record=false
--enable_transaction_sealing=false
--txn_max_apply_batch_records=100000
--dump_transactions=false
--TEST_transaction_allow_rerequest_status=true
--wait_for_relock_unblocked_txn_keys_ms=0
--encryption_counter_overflow_read_path_workaround=true
--TEST_encryption_use_openssl_compatible_counter_overflow=true
--encryption_counter_max=2147483647
--encryption_counter_min=0
--TEST_simulate_fs_create_failure=false
--enable_data_block_fsync=true
--fs_wal_dirs=/mnt/d0
--instance_uuid_override=
--num_cpus=0
--master_leader_rpc_timeout_ms=500
--memstore_arena_size_kb=64
--aggressive_compaction_for_read_amp=false
--TEST_max_write_waiters=2147483647
--automatic_compaction_extra_priority=50
--compaction_priority_start_bound=10
--compaction_priority_step_size=5
--dump_dbimpl_info=false
--fault_crash_after_rocksdb_flush=0
--flush_rocksdb_on_shutdown=true
--rocksdb_use_logging_iterator=false
--small_compaction_extra_priority=1
--use_priority_thread_pool_for_compactions=true
--use_priority_thread_pool_for_flushes=false
--TEST_rocksdb_crash_on_flush=false
--rocksdb_nothing_in_memtable_to_flush_sleep_ms=10
--rocksdb_release_mutex_during_wait_for_memtables_to_flush=true
--use_per_file_metadata_for_flushed_frontier=false
--TEST_allow_stop_writes=true
--verify_encrypted_meta_block_checksums=true
--cache_overflow_single_touch=true
--cache_single_touch_ratio=0.20000000000000001
--allow_preempting_compactions=true
--rocksdb_file_starting_buffer_size=8192
--rpc_acceptor_listen_backlog=128
--binary_call_parser_reject_on_mem_tracker_hard_limit=true
--rpc_throttle_threshold_bytes=1048576
--stream_compression_algo=0
--rpc_connection_timeout_ms=15000
--read_buffer_memory_limit=-5
--collect_end_to_end_traces=false
--print_trace_every=0
--rpc_dump_all_traces=false
--rpc_slow_query_threshold_ms=10000
--rpc_max_message_size=267386880
--io_thread_pool_size=4
--outbound_rpc_memory_limit=0
--rpc_default_keepalive_time_ms=65000
--rpc_queue_limit=10000
--rpc_workers_limit=1024
--socket_receive_buffer_size=0
--rpc_callback_max_cycles=100000000
--num_connections_to_server=8
--proxy_resolve_cache_ms=5000
--rpc_read_buffer_size=0
--linear_backoff_ms=1
--max_backoff_ms_exponent=16
--min_backoff_ms_exponent=7
--rpcs_shutdown_extra_delay_ms=5000
--rpcs_shutdown_timeout_ms=15000
--allow_insecure_connections=true
--cipher_list=
--ciphersuites=
--dump_certificate_entries=false
--ssl_protocols=
--verify_client_endpoint=false
--verify_server_endpoint=true
--TEST_enable_backpressure_mode_for_testing=false
--backpressure_recovery_period_ms=600000
--max_time_in_queue_ms=6000
--TEST_strand_done_inject_delay_ms=0
--TEST_delay_connect_ms=0
--TEST_yb_inbound_big_calls_parse_delay_ms=0
--enable_rpc_keepalive=true
--min_sidecar_buffer_size=16384
--callhome_collection_level=medium
--callhome_enabled=true
--callhome_interval_secs=3600
--callhome_tag=
--callhome_url=http://diagnostics.yugabyte.com
--web_log_bytes=1048576
--clock_skew_force_crash_bound_usec=60000000
--fail_on_out_of_range_clock_skew=true
--time_source=
--use_hybrid_clock=true
--rpc_bind_addresses=0.0.0.0
--rpc_server_allow_ephemeral_ports=false
--TEST_check_broadcast_address=true
--TEST_nodes_per_cloud=2
--TEST_public_hostname_suffix=.ip.yugabyte
--TEST_simulate_port_conflict_error=false
--generic_svc_num_threads=10
--generic_svc_queue_length=50
--num_reactor_threads=1
--yb_test_name=
--master_discovery_timeout_ms=3600000
--metrics_log_interval_ms=0
--placement_cloud=local
--placement_region=local
--placement_uuid=
--placement_zone=local1
--server_broadcast_addresses=
--server_dump_info_format=json
--server_dump_info_path=
--memory_limit_termination_threshold_pct=200
--total_mem_watcher_interval_millis=1000
--TEST_mini_cluster_mode=false
--webserver_compression_threshold_kb=4
--webserver_max_post_length_bytes=1048576
--webserver_zlib_compression_level=1
--webserver_authentication_domain=
--webserver_certificate_file=
--webserver_doc_root=
--webserver_enable_doc_root=true
--webserver_interface=
--webserver_num_worker_threads=50
--webserver_password_file=
--webserver_port=9000
--TEST_pause_and_skip_apply_intents_task_loop_ms=0
--apply_intents_task_injected_delay_ms=0
--enable_maintenance_manager=true
--maintenance_manager_history_size=8
--maintenance_manager_num_threads=1
--maintenance_manager_polling_interval_ms=250
--TEST_inject_mvcc_delay_add_leader_pending_ms=0
--TEST_mvcc_op_trace_num_items=32
--TEST_delay_execute_async_ms=0
--tablet_operation_memory_limit_mb=1024
--TEST_modify_flushed_frontier_snapshot_op=true
--consistent_restore=false
--TEST_tablet_inject_latency_on_apply_write_txn_ms=0
--TEST_tablet_pause_apply_write_ops=false
--TEST_preparer_batch_inject_latency_ms=0
--max_group_replicate_batch_size=16
--TEST_transaction_delay_status_reply_usec_in_tests=0
--transaction_abort_check_interval_ms=5000
--transaction_abort_check_timeout_ms=30000
--TEST_backfill_drop_frequency=0
--TEST_backfill_paging_size=0
--TEST_backfill_sabotage_frequency=0
--TEST_disable_adding_user_frontier_to_sst=false
--TEST_disable_getting_user_frontier_from_mem_table=false
--TEST_docdb_log_write_batches=false
--TEST_export_intentdb_metrics=false
--TEST_inject_sleep_before_applying_intents_ms=0
--TEST_pause_before_post_split_compaction=false
--TEST_slowdown_backfill_by_ms=0
--TEST_tablet_verify_flushed_frontier_after_modifying=false
--backfill_index_rate_rows_per_sec=0
--backfill_index_timeout_grace_margin_ms=-1
--backfill_index_write_batch_size=128
--cleanup_intents_sst_files=true
--delete_intents_sst_files=true
--disable_alter_vs_write_mutual_exclusion=false
--enable_pessimistic_locking=false
--intents_flush_max_delay_ms=2000
--num_raft_ops_to_force_idle_intents_db_to_flush=1000
--tablet_bloom_block_size=4096
--tablet_bloom_target_fp_rate=0.0099999997764825821
--tablet_do_compaction_cleanup_for_intents=true
--tablet_do_dup_key_checks=true
--tablet_enable_ttl_file_filter=false
--tablet_rocksdb_ops_quiet_down_timeout_ms=60000
--verify_index_rate_rows_per_sec=0
--verify_index_read_batch_size=128
--yql_allow_compatible_schema_versions=true
--ysql_transaction_abort_timeout_ms=900000
--TEST_dump_docdb_after_tablet_bootstrap=false
--TEST_dump_docdb_before_tablet_bootstrap=false
--TEST_fault_crash_during_log_replay=0
--TEST_play_pending_uncommitted_entries=false
--TEST_tablet_bootstrap_delay_ms=0
--force_recover_flushed_frontier=false
--skip_flushed_entries=true
--skip_remove_old_recovery_dir=false
--skip_wal_rewrite=true
--transaction_status_tablet_log_segment_size_bytes=4194304
--enable_tablet_orphaned_block_deletion=true
--TEST_delay_init_tablet_peer_ms=0
--cdc_min_replicated_index_considered_stale_secs=900
--propagate_safe_time=true
--wait_queue_poll_interval_ms=1000
--enable_history_cutoff_propagation=false
--history_cutoff_propagation_interval_ms=180000
--timestamp_history_retention_interval_sec=900
--TEST_inject_random_delay_on_txn_status_response_ms=0
--TEST_inject_txn_get_status_delay_ms=0
--avoid_abort_after_sealing_ms=20
--transaction_check_interval_usec=500000
--transaction_max_missed_heartbeat_periods=10
--transaction_resend_applying_interval_usec=5000000
--TEST_inject_load_transaction_delay_ms=0
--TEST_fail_in_apply_if_no_metadata=false
--TEST_transaction_ignore_applying_probability=0
--TEST_txn_participant_inject_latency_on_apply_update_txn_ms=0
--max_transactions_in_status_request=128
--transaction_min_running_check_delay_ms=50
--transaction_min_running_check_interval_ms=250
--transactions_cleanup_cache_size=256
--transactions_poll_check_aborted=true
--transactions_status_poll_interval_ms=500
--TEST_inject_status_resolver_complete_delay_ms=0
--TEST_inject_status_resolver_delay_ms=0
--TEST_tserver_disable_heartbeat=false
--heartbeat_interval_ms=1000
--heartbeat_max_failures_before_backoff=3
--heartbeat_rpc_timeout_ms=15000
--metrics_snapshotter_interval_ms=30000
--metrics_snapshotter_table_metrics_whitelist=rocksdb_sst_read_micros_sum,rocksdb_sst_read_micros_count
--metrics_snapshotter_tserver_metrics_whitelist=handler_latency_yb_client_read_local_sum,handler_latency_yb_client_read_local_count
--metrics_snapshotter_ttl_ms=604800000
--tserver_metrics_snapshotter_yb_client_default_timeout_ms=5000
--pg_client_session_expiration_ms=60000
--TEST_assert_reads_served_by_follower=false
--TEST_simulate_time_out_failures_msecs=0
--TEST_transactional_read_delay_ms=0
--parallelize_read_ops=true
--ysql_follower_reads_avoid_waiting_for_safe_time=true
--TEST_download_partial_wal_segments=false
--TEST_fault_crash_bootstrap_client_before_changing_role=0
--TEST_pause_rbs_before_download_wal=false
--TEST_simulate_long_remote_bootstrap_sec=0
--committed_config_change_role_timeout_sec=30
--remote_bootstrap_begin_session_timeout_ms=5000
--remote_bootstrap_end_session_timeout_sec=15
--remote_bootstrap_save_downloaded_metadata=false
--bytes_remote_bootstrap_durable_write_mb=8
--remote_boostrap_rate_limit_bytes_per_sec=0
--remote_bootstrap_max_chunk_size=67108864
--remote_bootstrap_rate_limit_bytes_per_sec=268435456
--TEST_fault_crash_leader_after_changing_role=0
--TEST_fault_crash_leader_before_changing_role=0
--TEST_fault_crash_on_handle_rb_fetch_data=0
--TEST_inject_latency_before_change_role_secs=0
--TEST_skip_change_role=false
--remote_bootstrap_change_role_timeout_ms=15000
--remote_bootstrap_idle_timeout_ms=180000
--remote_bootstrap_timeout_poll_period_ms=10000
--TEST_assert_reads_from_follower_rejected_because_of_staleness=false
--TEST_write_rejection_percentage=0
--max_rejection_delay_ms=5000
--max_stale_read_bound_time_ms=60000
--min_rejection_delay_ms=100
--sst_files_hard_limit=48
--sst_files_soft_limit=24
--TEST_pretend_memory_exceeded_enforce_flush=false
--db_block_cache_num_shard_bits=4
--db_block_cache_size_bytes=-1
--db_block_cache_size_percentage=10
--enable_block_based_table_cache_gc=false
--enable_log_cache_gc=true
--global_memstore_size_mb_max=2048
--global_memstore_size_percentage=10
--log_cache_gc_evict_only_over_allocated=true
--cql_proxy_bind_address=0.0.0.0:9042
--cql_proxy_webserver_port=12000
--enable_direct_local_tablet_server_call=true
--inbound_rpc_memory_limit=0
--pg_client_svc_queue_length=5000
--pgsql_proxy_bind_address=0.0.0.0:5433
--redis_proxy_bind_address=0.0.0.0:6379
--redis_proxy_webserver_port=11000
--tablet_server_svc_num_threads=64
--tablet_server_svc_queue_length=5000
--ts_admin_svc_num_threads=10
--ts_admin_svc_queue_length=50
--ts_consensus_svc_num_threads=64
--ts_consensus_svc_queue_length=5000
--ts_remote_bootstrap_svc_num_threads=10
--ts_remote_bootstrap_svc_queue_length=50
--tserver_enable_metrics_snapshotter=false
--tserver_yb_client_default_timeout_ms=60000
--cql_proxy_broadcast_rpc_address=
--enable_ysql=true
--start_cql_proxy=true
--start_pgsql_proxy=true
--start_redis_proxy=true
--TEST_alter_schema_delay_ms=0
--TEST_disable_post_split_tablet_rbs_check=false
--TEST_fail_alter_schema_after_abort_transactions=false
--TEST_fail_tablet_split_probability=0
--TEST_leader_stepdown_delay_ms=0
--TEST_pause_tserver_get_split_key=false
--TEST_respond_write_failed_probability=0
--TEST_rpc_delete_tablet_fail=false
--TEST_scanner_inject_latency_on_each_batch_ms=0
--TEST_tserver_noop_read_write=false
--TEST_txn_status_moved_rpc_force_fail=false
--TEST_txn_status_moved_rpc_handle_delay_ms=0
--TEST_txn_status_table_tablet_creation_delay_ms=0
--index_backfill_additional_delay_before_backfilling_ms=0
--index_backfill_upperbound_for_user_enforced_txn_duration_ms=65000
--index_backfill_wait_for_old_txns_ms=0
--max_wait_for_safe_time_ms=5000
--num_concurrent_backfills_allowed=1
--scanner_batch_size_rows=100
--scanner_default_batch_size_bytes=65536
--scanner_max_batch_size_bytes=8388608
--TEST_apply_tablet_split_inject_delay_ms=0
--TEST_crash_after_tablet_split_completed=false
--TEST_crash_before_apply_tablet_split_op=false
--TEST_crash_before_source_tablet_mark_split_done=false
--TEST_crash_if_remote_bootstrap_sessions_greater_than=0
--TEST_crash_if_remote_bootstrap_sessions_per_table_greater_than=0
--TEST_fault_crash_after_blocks_deleted=0
--TEST_fault_crash_after_cmeta_deleted=0
--TEST_fault_crash_after_rb_files_fetched=0
--TEST_fault_crash_after_wal_deleted=0
--TEST_fault_crash_in_split_after_log_copied=0
--TEST_fault_crash_in_split_before_log_flushed=0
--TEST_force_single_tablet_failure=false
--TEST_pause_apply_tablet_split=false
--TEST_simulate_already_present_in_remote_bootstrap=false
--TEST_skip_deleting_split_tablets=false
--TEST_skip_post_split_compaction=false
--TEST_sleep_after_tombstoning_tablet_secs=0
--cleanup_metrics_interval_sec=60
--cleanup_split_tablets_interval_sec=60
--enable_restart_transaction_status_tablets_first=true
--num_tablets_to_open_simultaneously=0
--post_split_trigger_compaction_pool_max_queue_size=16
--post_split_trigger_compaction_pool_max_threads=1
--read_pool_max_queue_size=128
--read_pool_max_threads=128
--skip_tablet_data_verification=false
--tablet_start_warn_threshold_ms=500
--verify_tablet_data_interval_sec=0
--tserver_master_addrs=yb-1.local:7100,yb-2.local:7100,yb-3.local:7100
--tserver_master_replication_factor=0
--tserver_heartbeat_metrics_add_drive_data=true
--tserver_heartbeat_metrics_interval_ms=5000
--TEST_promote_all_auto_flags=false
--use_icu_timezones=true
--use_libbacktrace=false
--trace_to_console=
--TEST_simulate_free_space_bytes=-1
--TEST_simulate_fs_without_fallocate=false
--never_fsync=false
--o_direct_block_alignment_bytes=4096
--o_direct_block_size_bytes=4096
--writable_file_use_fsync=false
--suicide_on_eio=true
--TEST_running_test=false
--dump_flags_xml=false
--dump_metrics_json=false
--enable_process_lifetime_heap_profiling=false
--heap_profile_path=/tmp/yb-tserver.41588
--help_auto_flag_json=false
--svc_queue_length_default=50
--fs_data_dirs=/mnt/d0
--stop_on_parent_termination=false
--fatal_details_path_prefix=
--log_filename=yb-tserver
--minicluster_daemon_id=
--ref_counted_debug_type_name_regex=
--default_memory_limit_to_ram_ratio=0.59999999999999998
--mem_tracker_log_stack_trace=false
--mem_tracker_logging=false
--mem_tracker_tcmalloc_gc_release_bytes=5063344
--mem_tracker_update_consumption_interval_us=2000000
--memory_limit_hard_bytes=0
--memory_limit_soft_percentage=85
--memory_limit_warn_threshold_percentage=98
--server_tcmalloc_max_total_thread_cache_bytes=33554432
--tcmalloc_max_free_bytes_percentage=10
--tserver_tcmalloc_max_total_thread_cache_bytes=33554432
--arena_warn_threshold_bytes=268435456
--allocator_aligned_mode=false
--metric_node_name=yb-1.local:9000
--metrics_retirement_age_ms=120000
--expose_metric_histogram_percentiles=true
--max_tables_metrics_breakdowns=2147483647
--dns_cache_expiration_ms=60000
--net_address_filter=ipv4_external,ipv4_all,ipv6_external,ipv6_non_link_local,ipv6_all
--rate_limiter_min_rate=1000
--rate_limiter_min_size=32768
--local_ip_for_outbound_sockets=
--socket_inject_short_recvs=false
--TEST_fail_write_pb_container=false
--protobuf_message_total_bytes_limit=535822336
--disable_clock_sync_error=true
--max_clock_skew_usec=500000
--max_clock_sync_error_usec=10000000
--non_graph_characters_percentage_to_use_hexadecimal_rendering=10
--lock_contention_trace_threshold_cycles=2000000
--enable_tracing=false
--print_nesting_levels=5
--sampled_trace_1_in_n=1000
--tracing_level=0
--use_monotime_for_traces=false
--rlimit_as=-1
--rlimit_cpu=-1
--rlimit_data=-1
--rlimit_fsize=-1
--rlimit_memlock=65536
--rlimit_nofile=1048576
--rlimit_nproc=12000
--rlimit_stack=8388608
--version_file_json_path=
--ycql_ldap_base_dn=
--ycql_ldap_bind_dn=****
--ycql_ldap_bind_passwd=****
--ycql_ldap_search_attribute=
--ycql_ldap_search_filter=
--ycql_ldap_server=
--ycql_ldap_tls=false
--ycql_ldap_user_prefix=
--ycql_ldap_user_suffix=
--ycql_ldap_users_to_skip_csv=****
--ycql_use_ldap=false
--cql_server_always_send_events=false
--display_bind_params_in_cql_details=true
--max_message_length=266338304
--rpcz_max_cql_batch_dump_count=4096
--rpcz_max_cql_query_dump_size=4096
--throttle_cql_calls_on_soft_memory_limit=true
--throttle_cql_calls_policy=0
--cql_nodelist_refresh_interval_secs=300
--cql_rpc_memory_limit=0
--cql_service_queue_length=10000
--cql_rpc_keepalive_time_ms=120000
--cql_processors_limit=-4000
--cql_service_max_prepared_statement_size_bytes=134217728
--cql_ybclient_reactor_threads=24
--password_hash_cache_size=64
--cql_system_query_cache_empty_responses=true
--cql_system_query_cache_stale_msecs=60000
--cql_system_query_cache_tables=
--cql_update_system_query_cache_msecs=0
--ycql_audit_excluded_categories=
--ycql_audit_excluded_keyspaces=system,system_schema,system_virtual_schema,system_auth
--ycql_audit_excluded_users=
--ycql_audit_included_categories=
--ycql_audit_included_keyspaces=
--ycql_audit_included_users=
--ycql_audit_log_level=ERROR
--ycql_enable_audit_log=false
--cql_prepare_child_threshold_ms=2000
--ycql_serial_operation_in_transaction_block=true
--cql_allow_static_column_index=false
--cql_raise_index_where_clause_error=false
--cql_table_is_transactional_by_default=false
--enable_uncovered_index_select=true
--ycql_allow_in_op_with_order_by=false
--allow_index_table_read_write=false
--use_cassandra_authentication=false
--ycql_cache_login_info=false
--ycql_require_drop_privs_for_truncate=false
--ycql_use_local_transaction_tables=false
--pg_client_heartbeat_interval_ms=10000
--TEST_user_ddl_operation_timeout_sec=0
--TEST_do_not_add_enum_sort_order=false
--TEST_ysql_ignore_add_fk_reference=false
--ysql_log_failed_docdb_requests=false
--ysql_wait_until_index_permissions_timeout_ms=3600000
--pg_yb_session_timeout_ms=600000
--use_node_hostname_for_local_tserver=false
--TEST_index_read_multiple_partitions=false
--TEST_inject_delay_between_prepare_ybctid_execute_batch_ybctid_ms=0
--TEST_pggate_ignore_tserver_shm=false
--TEST_yb_lwlock_crash_after_acquire_pg_stat_statements_reset=false
--TEST_ysql_disable_transparent_cache_refresh_retry=false
--pggate_master_addresses=
--pggate_rpc_timeout_secs=60
--pggate_tserver_shm_fd=-1
--pggate_ybclient_reactor_threads=2
--pgsql_rpc_keepalive_time_ms=0
--yb_enable_read_committed_isolation=false
--ysql_backward_prefetch_scale_factor=0.0625
--ysql_beta_feature_tablegroup=true
--ysql_beta_feature_tablespace_alteration=false
--ysql_beta_features=false
--ysql_disable_portal_run_context=false
--ysql_enable_update_batching=true
--ysql_max_read_restart_attempts=20
--ysql_max_write_restart_attempts=20
--ysql_non_txn_copy=false
--ysql_output_buffer_size=262144
--ysql_prefetch_limit=1024
--ysql_request_limit=1024
--ysql_select_parallelism=-1
--ysql_sequence_cache_minval=100
--ysql_serializable_isolation_for_ddl_txn=false
--ysql_session_max_batch_size=3072
--ysql_sleep_before_retry_on_txn_conflict=true
--ysql_suppress_unsupported_error=false
--TEST_ysql_oid_prefetch_adjustment=0
--pggate_num_connections_to_server=1
--ysql_client_read_write_timeout_ms=-1
--ysql_enable_reindex=false
--TEST_pg_collation_enabled=true
--pg_proxy_bind_address=
--pg_stat_statements_enabled=true
--pg_transactions_enabled=true
--pg_verbose_error_log=false
--pgsql_proxy_webserver_port=13000
--postmaster_cgroup=
--yb_backend_oom_score_adj=900
--yb_pg_terminate_child_backend=false
--ysql_datestyle=
--ysql_default_transaction_isolation=
--ysql_enable_auth=false
--ysql_hba_conf=****
--ysql_hba_conf_csv=****
--ysql_log_min_duration_statement=
--ysql_log_min_messages=
--ysql_log_statement=
--ysql_max_connections=0
--ysql_pg_conf=
--ysql_pg_conf_csv=
--ysql_timezone=
--redis_keys_threshold=10000
--redis_passwords_separator=,
--use_hashed_redis_password=true
--yedis_enable_flush=true
--redis_connection_soft_limit_grace_period_sec=60
--redis_max_batch=500
--redis_max_concurrent_commands=1
--redis_max_queued_bytes=134217728
--redis_max_read_buffer_size=134217728
--rpcz_max_redis_query_dump_size=4096
--redis_rpc_block_size=1048576
--redis_rpc_memory_limit=0
--redis_svc_queue_length=5000
--redis_rpc_keepalive_time_ms=0
--enable_redis_auth=true
--redis_callbacks_threadpool_size=64
--redis_max_command_size=265289728
--redis_max_value_size=67108864
--redis_password_caching_duration_ms=5000
--redis_safe_batch=true
--redis_service_yb_client_timeout_millis=3000
--flagfile=/opt/yugabyte/conf/tserver.conf
--fromenv=
--tryfromenv=
--undefok=
--tab_completion_columns=80
--tab_completion_word=
--help=false
--helpfull=false
--helpmatch=
--helpon=
--helppackage=false
--helpshort=false
--helpxml=false
--version=false
--alsologtoemail=
--alsologtostderr=false
--colorlogtostderr=false
--drop_log_memory=true
--log_backtrace_at=
--log_dir=/mnt/d0/yb-data/tserver/logs
--log_link=
--log_prefix=true
--log_prefix_include_pid=false
--logbuflevel=-1
--logbufsecs=30
--logemaillevel=999
--logfile_mode=436
--logmailer=/bin/mail
--logtostderr=false
--max_log_size=1800
--minloglevel=0
--stderrthreshold=3
--stop_logging_if_full_disk=false
--symbolize_stacktrace=true
--v=0
--vmodule=
"#.to_string();
        let result = parse_gflags(gflags);
        assert_eq!(result.len(), 829);
    }

    use crate::utility;
    #[test]
    fn integration_parse_gflags_master() {
        let mut stored_gflags: Vec<StoredGFlags> = Vec::new();
        let detail_snapshot_time = Local::now();
        let hostname = utility::get_hostname_master();
        let port = utility::get_port_master();

        let gflags = read_gflags(hostname.as_str(), port.as_str());
        add_to_gflags_vector(gflags, format!("{}:{}", hostname, port).as_str(), detail_snapshot_time, &mut stored_gflags);
        // the master must have gflags
        assert!(!stored_gflags.is_empty());
    }
    #[test]
    fn integration_parse_gflags_tserver() {
        let mut stored_gflags: Vec<StoredGFlags> = Vec::new();
        let detail_snapshot_time = Local::now();
        let hostname = utility::get_hostname_tserver();
        let port = utility::get_port_tserver();

        let gflags = read_gflags(hostname.as_str(), port.as_str());
        add_to_gflags_vector(gflags, format!("{}:{}", hostname, port).as_str(), detail_snapshot_time, &mut stored_gflags);
        // the tserver must have gflags
        assert!(!stored_gflags.is_empty());
    }
}