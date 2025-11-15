
### additional_p2p_trusted_ca_certs
- **Type:** `alloc::vec::Vec<ditto_types::value::Value>`
- **Default value:** []
- **Bounds:** No additional restrictions on the value
- **Remarks:** Additional trusted CA certificates for X.509 identity validation.  This parameter allows specifying additional Certificate Authority certificates that should be trusted when authenticating peers. Certificates should be provided as base64-encoded DER format strings.  This is primarily used to support migration scenarios where SharedKey Small Peers need to trust Big Peer certificates during a transition period.

### attachments_auto_fetch
- **Type:** `bool`
- **Default value:** false
- **Bounds:** No additional restrictions on the value
- **Remarks:** Automatically fetch attachments from other peers.  Set to true if this peer should automatically fetch attachments that match sync subscription results from other peers without a request from the SDK. Set to false if new attachments must wait for an SDK request to transit the network. (Default)

### attachments_availability_filter_enabled
- **Type:** `u64`
- **Default value:** 1
- **Bounds:** At least 0 (inclusive) and at most 1 (inclusive)
- **Remarks:** Controls the availability filter optimization  When enabled, peers exchange bloom filters indicating attachment availability. This speeds up fetches because a peer knows exactly which subset of connected peers may have data for an attachment. Probabilistically, this set should be much smaller.  Disabling this optimization will significantly slow down attachment transfers. There will be many more instances of [`AttachmentError::NotFound`](crate::blobs::msg::AttachmentError).  <https://en.wikipedia.org/wiki/Bloom_filter>

### attachments_clean_queue_max_batch_size
- **Type:** `u64`
- **Default value:** 50
- **Bounds:** At least 1 (inclusive) and at most 4294967295 (inclusive)
- **Remarks:** Controls the maximum number of queue entries processed in a single write transaction by [`Attachments::clean_queue`]  Decreasing this value may improve performance if there are write transaction timeouts.

### attachments_doc_link_max_batch_size
- **Type:** `u64`
- **Default value:** 100
- **Bounds:** At least 1 (inclusive) and at most 4294967295 (inclusive)
- **Remarks:** Maximum number of attachment links to process in a single write transaction in [`Attachments::add_doc_links`]  Decreasing this value may help if there are attachment transaction timeouts. Increasing this value can help if there is backpressure on the documents database created by the [`super::tasks::links_maintainer`].

### attachments_entry_cache_timeout_ms
- **Type:** `u64`
- **Default value:** 6000
- **Bounds:** No additional restrictions on the value
- **Remarks:** Evict attachment entries from the in-memory cache when they have not been used for at least this duration  The entry cache tracks basic metadata about attachments needed for attachment replication. Each entry is a read-write lock that synchronizes operations on that particular attachment.  This is intentionally the same as [`INFLIGHT_TIMEOUT_MS`](super::INFLIGHT_TIMEOUT_MS) for performance reasons.

### attachments_fill_from_shared_initial_backoff_ms
- **Type:** `ditto_configuration::types::Milliseconds`
- **Default value:** Milliseconds(1000)
- **Bounds:** No additional restrictions on the value
- **Remarks:** The initial backoff time when the shared storage backend indicates that an attachment does not exist  Note: this parameter is _NOT_ relevant to small peers, it only affects subscription server.  An exponential backoff with jitter strategy is used to control the volume of concurrent requests to shared blob storage. The default value is chosen arbitrarily. Increasing this value could reduce the number of concurrent requests being made to shared storage (i.e. S3).

### attachments_garbage_collect_delete_attachments_max_batch_size
- **Type:** `u64`
- **Default value:** 10
- **Bounds:** At least 1 (inclusive) and at most 4294967295 (inclusive)
- **Remarks:** Maximum number of attachments to delete in a single batch during [garbage collection](Attachments::garbage_collect)  Decreasing this value may help if there are attachment transaction timeouts. Increasing this value can help if there is backpressure on the documents database created by the [`super::tasks::links_maintainer`].

### attachments_garbage_collect_delete_links_max_batch_size
- **Type:** `u64`
- **Default value:** 100
- **Bounds:** At least 1 (inclusive) and at most 4294967295 (inclusive)
- **Remarks:** Maximum number of links to delete in a single batch while [cleaning links](Attachments::clean_links) during garbage collection.  Decreasing this may help if there is write transaction contention. Making this larger than [`GC_VALIDATE_LINKS_MAX_BATCH_SIZE`] has no effect.

### attachments_garbage_collect_validate_links_max_batch_size
- **Type:** `u64`
- **Default value:** 100
- **Bounds:** At least 1 (inclusive) and at most 4294967295 (inclusive)
- **Remarks:** Maximum number of links to validate in a single batch while [cleaning links](Attachments::clean_links) during garbage collection.  Decreasing this may help if there is read transaction contention. Making this smaller than [`GC_DEL_LINKS_MAX_BATCH_SIZE`] has no effect.

### attachments_gc_interval_secs
- **Type:** `u64`
- **Default value:** 600
- **Bounds:** No additional restrictions on the value
- **Remarks:** Controls how frequently the [`janitor`] runs [`Attachments::garbage_collect`]  The default interval, 10 minutes, is chosen arbitrarily. Decreasing this could seriously degrade performance if the peer has more than 1000 attachments, see [#7755](https://github.com/getditto/ditto/issues/7755).

### attachments_links_maintainer_enable_changes_fallback
- **Type:** `bool`
- **Default value:** true
- **Bounds:** No additional restrictions on the value
- **Remarks:** Controls a links maintainer feature on small peer for falling back to the [changed API][chgd] under high load.  Disabling this will can cause backpressure on the [store event receiver][ger] if there are many documents containing attachments.  [chgd]: ditto_small_peer_store::collection::Collection::changed [ger]: ditto_store::store::Store::get_event_receiver

### attachments_links_maintainer_link_attempt_failed_delay
- **Type:** `ditto_configuration::types::Milliseconds`
- **Default value:** Milliseconds(1000)
- **Bounds:** No additional restrictions on the value
- **Remarks:** When [`Attachments::add_doc_links`] fails, delay for this duration before making another attempt  The default is chosen arbitrarily. Increasing this may reduce write transaction pressure on attachments. However, when an attempt does fail, [`crate::application::Application::wait_attachments_observe`] may cause a fetch to wait until the next attempt succeeds.

### attachments_links_maintainer_max_buffered_links
- **Type:** `u64`
- **Default value:** 512
- **Bounds:** At least 1 (inclusive) and at most 4294967295 (inclusive)
- **Remarks:** Maximum number of links to buffer while waiting to call [`Attachments::add_doc_links`].  The default is chosen arbitrarily. Increasing this parameter means Ditto can buffer more links in-memory before causing [store event receiver][ger] backpressure. This is only really helpful if there are _many_ attachments being added to documents at the same time. Decreasing this parameter reduces the maximum possible links in-memory though this may cause backpressure on the [store event receiver][ger].  [ger]: ditto_store::store::Store::get_event_receiver

### attachments_links_maintainer_max_link_attempts
- **Type:** `u64`
- **Default value:** 10
- **Bounds:** At least 1 (inclusive)
- **Remarks:** Maximum number of times to retry a [`Attachments::add_doc_links`] call before bailing out  The default is chosen arbitrarily.

### attachments_max_fill_from_shared_attempts
- **Type:** `u64`
- **Default value:** 5
- **Bounds:** At least 1 (inclusive)
- **Remarks:** Number of attempts made to check if an attachment exists in shared storage (i.e. AWS S3)  Note: this parameter is _NOT_ relevant to small peers, it only affects subscription server.  This matters when there are multiple subscription servers running for an application. They can race with each other in a way that leads to unnecessary attachment fetches from small peers. The default value is chosen arbitrarily. Decreasing this value could improve startup performance if there are many (on the order of 1,000+) attachments actively referenced by documents.

### attachments_outbound_data_incomplete_backoff_enabled
- **Type:** `u64`
- **Default value:** 1
- **Bounds:** At least 0 (inclusive) and at most 1 (inclusive)
- **Remarks:** Controls the outbound incomplete data backoff optimization  When sending an attachment, it is possible that this peer does not have the entire attachment yet. Outbound backoff allows this peer to wait up to [`super::OUTBOUND_DATA_INCOMPLETE_BACKOFF_MS`] for additional data before returning [`AttachmentError::Incomplete`](crate::blobs::msg::AttachmentError).  Disabling this optimization will increase the outbound transfer error rate and make attachments messaging more noisy. However, it could improve time to attachment fetch completion if the mesh contains a mixture of high and low bandwidth links; a peer will try to retrieve the remainder of an attachment from a different peer instead of waiting [`super::INFLIGHT_TIMEOUT_MS`].

### attachments_peer_backoff_linger_interval_ms
- **Type:** `u64`
- **Default value:** 30000
- **Bounds:** No additional restrictions on the value
- **Remarks:** Period on which we will attempt to retrieve a backed off attachment that is still incomplete from a remote peer  When an attachment backoff has finished and the attachment is still incomplete, we use a "linger" timer to periodically re-notify this AttachmentsPeer that it could do an inbound transfer for the attachment.

### attachments_peer_inbound_availability_false_positive_ms
- **Type:** `u64`
- **Default value:** 30000
- **Bounds:** No additional restrictions on the value
- **Remarks:** Initial attachment backoff duration when we ask another peer for an attachment but it does not have it.  This can happen if the probabilistic data structure used by attachments reports a false positive.

### attachments_peer_inbound_buffer_flush_timeout_ms
- **Type:** `u64`
- **Default value:** 250
- **Bounds:** No additional restrictions on the value
- **Remarks:** The maximum amount of time to wait before flushing inbound attachment data that is buffered in-memory  When receiving an attachment, we buffer chunks of it in-memory to avoid excessive writes to the underlying blob storage. Lowering this timeout would sync data more frequently, reducing the amount of data lost if execution suddenly stopped.

### attachments_peer_inbound_error_backoff_ms
- **Type:** `u64`
- **Default value:** 10000
- **Bounds:** No additional restrictions on the value
- **Remarks:** Initial attachment backoff duration when an inbound error is received or the attachment fails validation  Specific errors are [`AttachmentError::NotFound`] or [`AttachmentError::Busy`]

### attachments_peer_inbound_incomplete_backoff_ms
- **Type:** `u64`
- **Default value:** 500
- **Bounds:** No additional restrictions on the value
- **Remarks:** Initial attachment backoff duration when the remote peer is missing requested data. Intentionally much shorter than [`INBOUND_ERROR_BACKOFF_MS`] so we can "keep up" with remote progress.

### attachments_peer_inflight_backoff_after_timeout_ms
- **Type:** `u64`
- **Default value:** 3000
- **Bounds:** No additional restrictions on the value
- **Remarks:** Initial attachment backoff duration when an inbound transfer times out

### attachments_peer_inflight_timeout_ms
- **Type:** `u64`
- **Default value:** 6000
- **Bounds:** No additional restrictions on the value
- **Remarks:** When transferring an attachment, it takes time to send a message between peers. This parameter represents the maximum time to wait for the next message during a transfer to be received/generated. If this time is exceeded, the transfer will be stopped.  Tuning this parameter to a higher value could sped up transfers between two peers with poor connectivity at the cost of the two areas discussed below.  For outbound transfers, this parameter ensures a high degree of concurrency because we cut off remote peers that are taking too long to communicate. We also protect the system from hangs if it is taking too long to generate data for an outbound transfer.  For inbound transfers, this parameter ensures an attachment is fetched at a reasonable speed -- we can stop a transfer if the remote peer is taking too long to give us data and get the rest of the data from a different peer.

### attachments_peer_max_backoff_duration_ms
- **Type:** `u64`
- **Default value:** 30000
- **Bounds:** No additional restrictions on the value
- **Remarks:** An inbound attachment transfer may be stopped for various reasons. When it is stopped, we back off on receiving an attachment from a particular remote peer. This parameter represents the maximum duration of a backoff.  Backoffs allows us to try receiving the attachment from another peer, wait for additional data, or wait for an error to maybe fix itself.

### attachments_peer_max_concurrent_outbound
- **Type:** `u64`
- **Default value:** 20
- **Bounds:** At least 1 (inclusive)
- **Remarks:** How many outbound attachments a peer can service simultaneously  It is unlikely that you would ever need to increase this parameter for a small peer since we would rarely ever have this many connections. Increasing this parameter could also lead to higher in-memory usage proportional to [`MAX_OUTBOUND_ATTACHMENT_READAHEAD`].  Decreasing this parameter could improve throughput if the mesh is very low bandwidth since a peer would focus on a select few outbound transfers.  Historical note: testing an iPad Pro peer, 100 caused a crash, 40 was okay.

### attachments_peer_max_outbound_attachment_readahead
- **Type:** `u64`
- **Default value:** 65536
- **Bounds:** At least 1 (inclusive)
- **Remarks:** The maximum amount of data to buffer in-memory when generating attachment chunks for an outbound transfer  Increasing this parameter will result in higher memory usage but could speed up transfers if the underlying storage media performs random reads slowly (i.e. a hard drive).

### attachments_peer_outbound_data_incomplete_backoff_ms
- **Type:** `u64`
- **Default value:** 500
- **Bounds:** No additional restrictions on the value
- **Remarks:** Outbound backoff duration when available data is exhausted  During an outbound transfer, it is possible that this peer does not have all the data for an attachment. Once that available data is exhausted, this peer sets up a backoff in the hopes that additional data may be received from another peer, unblocking this transfer.  To be effective, it should be kept greater than [`INBOUND_BUFFER_FLUSH_TIMEOUT_MS`] and less than [`INFLIGHT_TIMEOUT_MS`].

### babel_ensured_message_count
- **Type:** `u8`
- **Default value:** 3
- **Bounds:** At least 0 (inclusive) and at most 5 (inclusive)
- **Remarks:** Number of times to send a message to ensure it was received by this node's neighbors.  ACKs are optional in babel, message repetition is a means of ensuring statistical likelihood that a message was received.

### babel_ensured_message_interval_sec
- **Type:** `ditto_configuration::types::Seconds`
- **Default value:** Seconds(2)
- **Bounds:** No additional restrictions on the value
- **Remarks:** How long to wait before resending a message to ensure it is received by this node's neighbors.  ACKs are optional in babel, message repetition is a means of ensuring statistical likelihood that a message was received.

### babel_full_update_interval_sec
- **Type:** `ditto_configuration::types::Seconds`
- **Default value:** Seconds(16)
- **Bounds:** No additional restrictions on the value
- **Remarks:** How often should the babel router send a full update to its neighbors.

### babel_hello_interval_sec
- **Type:** `ditto_configuration::types::Seconds`
- **Default value:** Seconds(4)
- **Bounds:** No additional restrictions on the value
- **Remarks:** How often should the babel router send hello messages to its neighbors.

### babel_ihu_expiry_factor
- **Type:** `f64`
- **Default value:** 4.0
- **Bounds:** At least 1 (inclusive)
- **Remarks:** How many times a neighbor's advertised IHU interval to wait before considering that neighbor unreachable.

### babel_ihu_interval_sec
- **Type:** `ditto_configuration::types::Seconds`
- **Default value:** Seconds(12)
- **Bounds:** No additional restrictions on the value
- **Remarks:** How often should the babel router send "I Heard You" messages to the neighbors it has received hellos from.  Babel RFC recommends this be 3x hello interval.

### babel_route_expiry_factor
- **Type:** `f64`
- **Default value:** 4.0
- **Bounds:** At least 1 (inclusive)
- **Remarks:** How many times the expected route update interval to wait before expiring that route.

### babel_route_request_factor
- **Type:** `f64`
- **Default value:** 0.75
- **Bounds:** At least 0 (inclusive) and at most 1 (not including 1)
- **Remarks:** What percentage of `babel_route_expiry_factor` to wait before sending an explicit route request for the route.

### babel_seqno_request_send_count
- **Type:** `u8`
- **Default value:** 3
- **Bounds:** No additional restrictions on the value
- **Remarks:** How many times to send a seqno request before giving up on a response.

### babel_seqno_request_start_hop_count
- **Type:** `u8`
- **Default value:** 64
- **Bounds:** No additional restrictions on the value
- **Remarks:** The starting hop count for a seqno request.

### babel_seqno_resend_interval_sec
- **Type:** `ditto_configuration::types::Seconds`
- **Default value:** Seconds(2)
- **Bounds:** No additional restrictions on the value
- **Remarks:** How long to wait before resending a local seqno request that has not yet been responded to.

### babel_sm_poll_interval_msec
- **Type:** `ditto_configuration::types::Milliseconds`
- **Default value:** Milliseconds(100)
- **Bounds:** No additional restrictions on the value
- **Remarks:** Rate that the babel router should poll for outgoing messages.

### babel_source_keepalive_sec
- **Type:** `ditto_configuration::types::Seconds`
- **Default value:** Seconds(180)
- **Bounds:** No additional restrictions on the value
- **Remarks:** How long to keep a source of routing information alive without receiving a message.

### blob_session_request_timeout_secs
- **Type:** `ditto_configuration::types::Seconds`
- **Default value:** Seconds(120)
- **Bounds:** No additional restrictions on the value
- **Remarks:** Maximum time allowed for a session to either handle a message or generate the next message. This is an arbitrarily high timeout that should never be reached during normal operation. If it _is_ reached then we should abort so that the Channel can be dropped.

### days_between_reaping
- **Type:** `u64`
- **Default value:** 1
- **Bounds:** At least 1 (inclusive)
- **Remarks:** Tombstone reaping is an infrequent background task concerned with automatic cleanup of tombstones that have expired their TTLs.  This parameter specifies the number of days between sweeps of the database by the reaper process.  By default we only check for expired tombstones once a day, but this parameter allows us to check less frequently. This would mainly be desirable for performance reasons; if you want to have tombstones expire more slowly, that can be achieved more directly by modifying the TTL parameter.  For performance reasons, we don't currently allow the tombstone reaper to be triggered more often than once per day. This constraint also simplifies configuration of time-of-day preferences for what time of day the user wants the reaping to occur.

### device_disk_observer_is_enabled
- **Type:** `bool`
- **Default value:** true
- **Bounds:** No additional restrictions on the value
- **Remarks:** Globally enable or disable device disk observation.  Device disk observation is a minor background task that periodically collects some high-level information about a device's disk, and specifically Ditto's disk usage. This information is stored in the system info table and surfaced on the device dashboard, as part of making the behavior of a deployed Ditto application observable.  The collected information has no effect on other Ditto functionality.

### disk_usage_monitor_interval
- **Type:** `ditto_configuration::types::Milliseconds`
- **Default value:** Milliseconds(0)
- **Bounds:** No additional restrictions on the value
- **Remarks:** The frequency at which the disk usage monitoring task runs.  This task feeds data to both small peer info (via the "device disk observer" feature) and the SDK disk usage callback.  A value of 0 (which is the default) allows the task to dynamically adjust the interval based on the cost of the underlying computation, i.e. run it more frequently if it's cheap, and less frequently if it becomes costly.

### doc_id_filter_error_rate
- **Type:** `f64`
- **Default value:** 0.01
- **Bounds:** At least 0 (inclusive) and at most 0.5 (inclusive)
- **Remarks:** The filter error rate to use when creating the [`QFilter`] that backs our document ID filter.  This is just a hint for tuning performance; see the [`QFilter`] documentation for more info.

### doc_id_filter_initial_capacity
- **Type:** `u64`
- **Default value:** 0
- **Bounds:** No additional restrictions on the value
- **Remarks:** The initial capacity to use when creating the [`QFilter`] that backs our document ID filter.  This is just a hint for tuning performance; see the [`QFilter`] documentation for more info.

### doc_id_filter_max_capacity
- **Type:** `u64`
- **Default value:** 1000000
- **Bounds:** At least 100000 (inclusive)
- **Remarks:** System parameter controlling the maximum number of documents that can be synced.  This is a hint for the maximum number of docs allowed to be synced.

### doc_sync_allow_rkyv_diffs
- **Type:** `bool`
- **Default value:** true
- **Bounds:** No additional restrictions on the value
- **Remarks:** Controls whether request Rkyv-encoded diffs to be sent in doc sync updates.  Rkyv provides better deserialization performance compared to legacy diff encodings.  A peer that has this system paramter set to true will send a protocol flag to any peers it syncs with, telling the other peers that they accept rkyv diffs.

### doc_sync_initial_update_diff_limit
- **Type:** `u32`
- **Default value:** 500
- **Bounds:** No additional restrictions on the value
- **Remarks:** Governs re-synchronization of reconnecting sessions where the local peer has built up a large backlog of changes to send.  When a session reconnects, the first time it attempts to create a diff-bearing update, it will apply this limit to that update. If more diff than this limit need to be sent, the update will be aborted, and a re-exchange of document summaries will be requested.  This limit is only applied to the _first_ post-reconnect diff-bearing update. After the document summary re-exchange completes, subsequent updates will send as many diffs as are needed.

### doc_sync_redundancy_backoff_delay_ms
- **Type:** `ditto_configuration::types::Milliseconds`
- **Default value:** Milliseconds(1000)
- **Bounds:** No additional restrictions on the value
- **Remarks:** The number of milliseconds to delay generating the next document sync update if the previous update exceeded the [redundancy backoff threshold](DOC_SYNC_REDUNDANCY_BACKOFF_THRESHOLD).  If the value is set to zero, then redundancy backoff is disabled.

### doc_sync_redundancy_backoff_threshold
- **Type:** `f64`
- **Default value:** 0.6
- **Bounds:** At least 0 (inclusive) and at most 1 (inclusive)
- **Remarks:** The proportion of fully-redundant diffs sent in a single document sync update that will cause the document sync engine to delay generating the next update.  The backoff delay is controlled by [`DOC_SYNC_REDUNDANCY_BACKOFF_DELAY_MS`].

### doc_sync_resync_on_reconnect_mins
- **Type:** `ditto_configuration::types::Minutes`
- **Default value:** Minutes(360)
- **Bounds:** No additional restrictions on the value
- **Remarks:** Governs re-synchronization of reconnecting sessions that have not seen inbound diffs within the specified number of minutes. When a session reconnects, if the local peer has not received inbound diffs in that session for longer than this period, it will initiate session sync and re-share document summaries before continuing to receive diffs.  This behavior is intended to address a tendency for peers to over share redundant data after reconnecting with each other after having spent time syncing with the broader mesh via other peers. It should be set lower for applications that tend to run in meshes with high nearest-neighbor variability. In applications where each peer generally only connects to one other, a longer period may be appropriate.  If this parameter is set to zero, then all sessions (with peers that support Session Sync) will initiate Session Sync on reconnect.

### doc_sync_retain_sessions_post_eviction
- **Type:** `bool`
- **Default value:** true
- **Bounds:** No additional restrictions on the value
- **Remarks:** Controls whether eviction cleanup is enabled for session garbage collection.  When enabled (default), post-eviction session cleanup attempts to preserve high-level metadata about non-expired document sync sessions for the sake of tracking sync status with the remote peers.  When disabled, post-eviction session cleanup treats all non-connected sessions as though they had exceeded the session TTL, and deletes them entirely. This provides a more aggressive cleanup approach that may be preferred in resource-constrained environments, or where tracking remote sync status is not important.  This parameter exists to provide flexibility in managing the trade-off between memory usage and session state preservation, allowing applications to choose the appropriate cleanup strategy based on their specific deployment requirements.

### doc_sync_session_activation_timeout_secs
- **Type:** `ditto_configuration::types::Seconds`
- **Default value:** Seconds(120)
- **Bounds:** No additional restrictions on the value
- **Remarks:** Maximum time allowed for a session to activate when attempting to connect. This is an arbitrary time out that is intended to allow lingering activity from a prior session to finish, and free up the session metadata, allowing the new session to activate, rather than immediately breaking the connection.  If it _is_ reached then we should abort so that the Channel can be dropped and the VC can shut down.

### document_size_hard_limit_bytes
- **Type:** `u64`
- **Default value:** 5242880
- **Bounds:** No additional restrictions on the value
- **Remarks:** The maximum document size in bytes before we start logging errors.  *NOTE* In the future, we will no longer just log errors; Ditto will actually reject documents that are bigger than this limit.  Creating large documents can have negative performance impacts. It's also not uncommon for application bugs to create documents that continue to grow in size over time. The limit [`DOCUMENT_SIZE_SOFT_LIMIT_BYTES`] is intended to warn users if documents start to grow too large. If they continue to grow to even larger sizes, this limit will be triggered, and a more severe log message will indicate the hard limit has been hit.  It is *very important* that this limit be consistent across all peers! For now, inconsistency in the hard limit will just lead to inconsistent logging behavior, but in the future inconsistencies in this setting could cause some peers to continue trying to replicate documents that other peers reject outright.  Note that this limit is only read at app startup, so in practice this will need to be set via environment variables prior to starting Ditto (or potentially via a persistent config layer in the future). This may change in the future, though, as there's no fundamental reason we couldn't re-read this at runtime; the code to do this simply hasn't been implemented yet.

### document_size_soft_limit_bytes
- **Type:** `u64`
- **Default value:** 262144
- **Bounds:** No additional restrictions on the value
- **Remarks:** The maximum document size in bytes before we start logging warnings.  Creating large documents can have negative performance impacts. It's also not uncommon for application bugs to create documents that continue to grow in size over time. This setting allows users to get an early warning if documents start growing beyond the given size threshold.  It is *highly recommended* that this parameter be set to the same value on all peers. If the value is not consistent, some peers will log warnings for the same document that other peers will silently accept.  Note that this limit is only read at app startup, so in practice this will need to be set via environment variables prior to starting Ditto (or potentially via a persistent config layer in the future). This may change in the future, though, as there's no fundamental reason we couldn't re-read this at runtime; the code to do this simply hasn't been implemented yet.

### dql_concurrent_request_limit
- **Type:** `u64`
- **Default value:** 0
- **Bounds:** No additional restrictions on the value
- **Remarks:** The maximum number of concurrent requests to run. (0 = disabled)

### dql_concurrent_request_wait_timeout_ms
- **Type:** `u64`
- **Default value:** 60000
- **Bounds:** No additional restrictions on the value
- **Remarks:** The maximum time (in milliseconds) for a held request to wait before giving up.

### dql_default_directives
- **Type:** `alloc::collections::btree::map::BTreeMap<compact_str::CompactString, ditto_types::value::Value>`
- **Default value:** {}
- **Bounds:** No additional restrictions on the value
- **Remarks:** The default DQL planner directives to use for all statements

### dql_enable_preview_mode
- **Type:** `bool`
- **Default value:** false
- **Bounds:** No additional restrictions on the value
- **Remarks:** Enables the use of preview features.

### dql_index_default_include_missing
- **Type:** `bool`
- **Default value:** true
- **Bounds:** No additional restrictions on the value
- **Remarks:** The default handling of MISSING values when user defined index keys don't explicitly state the desired behaviour

### dql_object_default
- **Type:** `alloc::string::String`
- **Default value:** "REGISTER"
- **Bounds:** PredicateValidator with the following:
Values must satisfy:
(Must equal 'REGISTER' (case insensitive) OR Must equal 'MAP' (case insensitive))

- **Remarks:** This flag is intended for advanced use. You should probably use [`DQL_STRICT_MODE`] under most circumstances. This system parameter allows you to opt-in into non-strict mode in a gradual manner. Changing the value for this parameter has no effect if `DQL_STRICT_MODE` is already set to `false`.  Changing `DQL_OBJECT_DEFAULT` allows you to specify the default CRDT type to use for objects in `INSERT` and `UPDATE` statements. Valid options are 'REGISTER' (the default) and 'MAP.  Changing the value of this parameter will only have effect when the [`DQL_SELECT_STRICT_MODE`] has been set to `false`.

### dql_request_history_log_dump_limit
- **Type:** `u64`
- **Default value:** 18446744073709551615
- **Bounds:** No additional restrictions on the value
- **Remarks:** The maximum number of request history cache entries to log for any one interval

### dql_request_history_qualifiers
- **Type:** `alloc::collections::btree::map::BTreeMap<compact_str::CompactString, ditto_types::value::Value>`
- **Default value:** {}
- **Bounds:** No additional restrictions on the value
- **Remarks:** The request history cache qualifiers (criteria for addition to the cache)

### dql_request_history_size
- **Type:** `u64`
- **Default value:** 4096
- **Bounds:** No additional restrictions on the value
- **Remarks:** The size of the request history cache (in requests)

### dql_restrict_subscriptions
- **Type:** `bool`
- **Default value:** true
- **Bounds:** No additional restrictions on the value
- **Remarks:** With this parameter set to true, subscriptions will not allow SELECTs with LIMIT or ORDER BY

### dql_select_strict_mode
- **Type:** `bool`
- **Default value:** true
- **Bounds:** No additional restrictions on the value
- **Remarks:** This flag is intended for advanced use. You should probably use [`DQL_STRICT_MODE`] under most circumstances. This system parameter allows you to opt-in into non-strict mode in a gradual manner. Changing the value for this parameter has no effect if `DQL_STRICT_MODE` is already set to `false`.  With `DQL_SELECT_STRICT_MODE` mode enabled, DQL works as it used to in 4.10 or below: fields not defined in the collection definition are treated as registers.  Disabling `DQL_SELECT_STRICT_MODE`` mode enables new functionality:  - The type of fields not defined in the collection definition is determined at run-time. When a field has multiple possible types, the most recently updated type is chosen. The main consequence of this is that is no longer necessary to define any types in a collection definition. `SELECT` queries will return and display all fields by default. This matches the behavior of the legacy query language.

### dql_statement_cache_size
- **Type:** `u64`
- **Default value:** 4096
- **Bounds:** No additional restrictions on the value
- **Remarks:** The size of the shared statement cache (in statements)

### dql_strict_mode
- **Type:** `bool`
- **Default value:** false
- **Bounds:** No additional restrictions on the value
- **Remarks:** With strict mode enabled, DQL works as it used to in 4.10 or below: fields not defined in the collection definition are treated as registers, and objects in INSERT and UPDATE statements are treated as registers.  Disabling strict mode enables new functionality:  - The type of fields not defined in the collection definition is determined at run-time. When a field has multiple possible types, the most recently updated type is chosen. The main consequence of this is that is no longer necessary to define any types in a collection definition. `SELECT` queries will return and display all fields by default. This matches the behavior of the legacy query language.  - Objects in INSERT and UPDATE statements are treated as maps. This matches the behavior of the legacy query language.

### dql_use_legacy_projection
- **Type:** `bool`
- **Default value:** true
- **Bounds:** No additional restrictions on the value
- **Remarks:** Reverts Select * projection format to legacy mode.

### enable_attachment_permission_checks
- **Type:** `bool`
- **Default value:** false
- **Bounds:** No additional restrictions on the value
- **Remarks:** Specifies whether attachment fetching has to undergo strict permissions checks.  Setting this to `true` results in `AttachmentFetcher` callbacks technically being able to stop being called at all in certain scenarios, requiring some external timeout mechanism to bail out of it, lest hanging ensue.  It defaults to `false`, which works around this issue by making attachment-fetching more lenient w.r.t. permission-checking, with a simpler model: if a peer witnesses a document (to which it thus had read access to) with an attachment inside (the so-called `AttachmentToken`), it is deemed that that peer has thereby obtained read access to the attachment byte contents (the "blob"), from there onwards.

### enable_doc_sync_protocol_trace
- **Type:** `bool`
- **Default value:** false
- **Bounds:** No additional restrictions on the value
- **Remarks:** Controls whether this peer will trace document sync protocol messages and update file records.  When enabled, detailed information about protocol messages (type, direction, size) and update file records (aggregated counts for high-frequency records, full contents for low-frequency records) will be logged at INFO level.  This is intended for debugging synchronization issues and understanding protocol behavior. Performance impact should be minimal when tracing is disabled.

### enable_reaper_preferred_hour_scheduling
- **Type:** `bool`
- **Default value:** false
- **Bounds:** No additional restrictions on the value
- **Remarks:** Tombstone reaping is an infrequent background task concerned with automatic cleanup of tombstones that have expired their TTLs.  If this is enabled, we'll try to schedule the tombstone reaping process to occur during the hour specified in the [`REAPER_PREFERRED_HOUR`] parameter.  This is disabled by default. When disabled, we default to running tombstone reaping shortly after startup, and then repeat every [`DAYS_BETWEEN_REAPING`] days, regardless of the time of day.  Enabling this setting does not specify that tombstone reaping will be triggered at a particular point during that hour; only that we will attempt to reap tombstones at some point during that hour, assuming Ditto is running during that time.  If preferred-hour scheduling is in use, we will still wait for [`DAYS_BETWEEN_REAPING`] days between triggering tombstone reaping cycles.  Note that if Ditto is not running during the preferred hour, tombstone reaping will be skipped that day.  Setting this to `true` on a WASM-based platform currently has no effect; preferred-hour scheduling is not supported under WASM.

### enable_remote_query
- **Type:** `bool`
- **Default value:** true
- **Bounds:** No additional restrictions on the value
- **Remarks:** Enables the Remote Query feature on this peer.  Regardless of whether this flag is enabled, the remote query service will still be started, but if this flag is set to `false`, then all requests to the remote query service will be denied.

### enable_selective_eviction_filters
- **Type:** `bool`
- **Default value:** true
- **Bounds:** No additional restrictions on the value
- **Remarks:** Controls whether this peer will use [probabilistic filters] to manage the deletion of document summary metadata, when synchronizing after a local eviction.  When enabled, this peer will send probabilistic filters to its remote peers, in an attempt to minimize unnecessary metadata deletion and resynchronization. This decreases synchronization messaging, but requires more computational overhead at the evicting peer.  When disabled, the peers will discard all summary metadata after eviction, which can be expected to cause a delay in subsequent synchronization.  [probabilistic filters]: <https://en.wikipedia.org/wiki/Bloom_filter>

### encrypted_blob_store_default_block_size
- **Type:** `u64`
- **Default value:** 4064
- **Bounds:** At least 1 (inclusive) and at most 4294967295 (inclusive)
- **Remarks:** Sets the default [block size](https://en.wikipedia.org/wiki/Block_size_%28cryptography%29) when encrypting blobs.  This parameter is only relevant to small peers that pass an encryption passphrase for encrypting Ditto data on-disk. It is used when creating new blobs or performing a key rotation. Writing to an existing blob will use the block size from when it was created instead.  If you decide to change this parameter, note that:  - The default size ensures a block is 4096 bytes long -- 32 bytes of metadata, 4064 bytes of data - Blobs stored by [`EncryptedBlobStore`] have a minimum on-disk size of 1 block. For example, setting the block size to 1MB means each blob needs at least 1MB of space

### established_physical_connection_setup_timeout
- **Type:** `ditto_configuration::types::Seconds`
- **Default value:** Seconds(60)
- **Bounds:** No additional restrictions on the value
- **Remarks:** Timeout for setting up an established physical connection

### example_array_parameter
- **Type:** `alloc::vec::Vec<ditto_types::value::Value>`
- **Default value:** []
- **Bounds:** No additional restrictions on the value
- **Remarks:** An example system parameter of type `ValueArray`. This can be used for testing the parameter store, but has no effect on Ditto itself.

### example_bool_parameter
- **Type:** `bool`
- **Default value:** true
- **Bounds:** No additional restrictions on the value
- **Remarks:** An example system parameter of type `bool`. This can be used for testing the parameter store, but has no effect on Ditto itself.

### example_duration_parameter
- **Type:** `ditto_configuration::types::Milliseconds`
- **Default value:** Milliseconds(200)
- **Bounds:** No additional restrictions on the value
- **Remarks:** An example system parameter of type `Milliseconds`. This can be used for testing the parameter store, but has no effect on Ditto itself.

### example_map_parameter
- **Type:** `alloc::collections::btree::map::BTreeMap<compact_str::CompactString, ditto_types::value::Value>`
- **Default value:** {}
- **Bounds:** No additional restrictions on the value
- **Remarks:** An example system parameter of type `ValueObject`. This can be used for testing the parameter store, but has no effect on Ditto itself.

### example_parameter
- **Type:** `u64`
- **Default value:** 42
- **Bounds:** At least 1 (inclusive) and at most 1024 (not including 1024)
- **Remarks:** An example system parameter. This one can be used for testing the parameter store, but has no effect on Ditto itself.

### example_string_parameter
- **Type:** `alloc::string::String`
- **Default value:** "default"
- **Bounds:** No additional restrictions on the value
- **Remarks:** An example system parameter of type `String`. This can be used for testing the parameter store, but has no effect on Ditto itself.

### filelock_developer_mode
- **Type:** `bool`
- **Default value:** false
- **Bounds:** No additional restrictions on the value
- **Remarks:** Ditto acquires a file lock on startup, to prevent multiple instances running atop the same data.  This parameter tells Ditto to completely ignore the lock. This can sometimes prevent Ditto from interfering with development workflows that involve restarting a Ditto app abruptly. It should never be used in a production setting!  Note: changes to this parameter only take effect when Ditto is restarted.

### filelock_retry_delay
- **Type:** `ditto_configuration::types::Milliseconds`
- **Default value:** Milliseconds(200)
- **Bounds:** At least 1 (inclusive)
- **Remarks:** Ditto acquires a file lock on startup, to prevent multiple instances running atop the same data.  This parameter sets the delay between retries—at most FILELOCK_RETRY_MAXIMUM retries—if the lock cannot be immediately acquired.  Note: changes to this parameter only take effect when Ditto is restarted.

### filelock_retry_maximum
- **Type:** `usize`
- **Default value:** 5
- **Bounds:** At least 1 (inclusive)
- **Remarks:** Ditto acquires a file lock on startup, to prevent multiple instances running atop the same data.  This parameter sets the maximum number of times Ditto will retry acquiring the lock—waiting for FILELOCK_RETRY_DELAY between retries—if the lock cannot be immediately acquired.  Note: changes to this parameter only take effect when Ditto is restarted.

### live_query_duplicate_warning_threshold
- **Type:** `u64`
- **Default value:** 5
- **Bounds:** No additional restrictions on the value
- **Remarks:** Warn about a possible resource leak, during live query registration, if there are at least this many identical live queries already registered.

### live_query_system_collection_refresh_interval
- **Type:** `ditto_configuration::types::Milliseconds`
- **Default value:** Milliseconds(500)
- **Bounds:** No additional restrictions on the value
- **Remarks:** Governs the frequency at which a live query targetting a virtual system collection will refresh its results. Does not affect live queries against the document store.

### live_query_total_warning_threshold
- **Type:** `u64`
- **Default value:** 100
- **Bounds:** No additional restrictions on the value
- **Remarks:** Warn about a possible resource leak, during live query registration, if the total number of live queries exceeds this threshold.

### max_keys_per_request
- **Type:** `u64`
- **Default value:** 1024
- **Bounds:** At least 1 (inclusive)
- **Remarks:** The maximum number of keys that can be requested in a single query.  If this is exceeded in an API call, the call will return [`StoreError::KeysLimitError`]. Be careful adjusting this, as making it higher could theoretically cause us to bump up against heretofore unknown limits elsewhere in the backend.  The lower bound is set to 1 since setting this to 0 would make it impossible to query individual keys, but in practice it would be very strange to set such a low value.

### max_open_remote_query_requests
- **Type:** `u64`
- **Default value:** 1000
- **Bounds:** At most 536870911 (inclusive)
- **Remarks:** Sets the number of maximum concurrent remote query requests that can be executed simultaneously against this peer.  If this limit is exceeded, additional requests will be blocked and forced to wait until a slot opens up.  Note that changes to this parameter *are* respected at runtime, but lowering the limit at runtime may not take effect immediately; any requests that are already running or waiting to start will still use the old limit. Increasing the limit, however, will take effect immediately.

### mesh_chooser_avoid_redundant_bluetooth
- **Type:** `bool`
- **Default value:** false
- **Bounds:** No additional restrictions on the value
- **Remarks:** Avoid creating Bluetooth connections with peers who have a better connection into the mesh

### mesh_chooser_churn_interval_ms
- **Type:** `ditto_configuration::types::Milliseconds`
- **Default value:** Milliseconds(120000)
- **Bounds:** No additional restrictions on the value
- **Remarks:** The minimum interval between performing churns for a particular connection type.  A "churn" is a deliberate disconnection of a long-established peer in order to free up a slot for another peer we haven't spoke for a long time (or ever). Apart from general randomness, this is Ditto's key protection against random peer-to-peer selections resulting in permanent islanding

### mesh_chooser_churn_min_peer_age_ms
- **Type:** `ditto_configuration::types::Milliseconds`
- **Default value:** Milliseconds(60000)
- **Bounds:** No additional restrictions on the value
- **Remarks:** All peers on a given client ConnectionType must be at least this old to initiate a churn

### mesh_chooser_cleanup_interval_ms
- **Type:** `ditto_configuration::types::Milliseconds`
- **Default value:** Milliseconds(600000)
- **Bounds:** No additional restrictions on the value
- **Remarks:** How often to prune old entries from the MeshChooser internal state

### mesh_chooser_connection_retry_cooldown_ms
- **Type:** `ditto_configuration::types::Milliseconds`
- **Default value:** Milliseconds(5000)
- **Bounds:** No additional restrictions on the value
- **Remarks:** Minimum frequency between making connection attempts to the same peer (on a given transport)

### mesh_chooser_max_active_ble_clients
- **Type:** `u64`
- **Default value:** 2
- **Bounds:** No additional restrictions on the value
- **Remarks:** Maximum number of BLE clients a peer will connect to at a time If your use case includes other non-Ditto BLE peripherals, you might want to set this number lower to conserve more of the hardware's capacity for non-Ditto purposes.  Linux & iOS use a larger default value of 3

### mesh_chooser_max_wlan_clients
- **Type:** `u64`
- **Default value:** 4
- **Bounds:** No additional restrictions on the value
- **Remarks:** Maximum number of outgoing connections to all LAN/WiFi/AWDL peers.

### mesh_chooser_peer_block_time_ms
- **Type:** `ditto_configuration::types::Milliseconds`
- **Default value:** Milliseconds(1800000)
- **Bounds:** No additional restrictions on the value
- **Remarks:** How long peers should be blocked for

### metrics_exporter_integration_metrics_rs
- **Type:** `bool`
- **Default value:** false
- **Bounds:** No additional restrictions on the value
- **Remarks:** Enables or disables the integration with the metrics-rs ecosystem.  When enabled, this integration allows Ditto's metrics system to ingest metrics from libraries and components that use the metrics-rs crate. This provides compatibility with the broader Rust ecosystem's metrics infrastructure, allowing third-party libraries' metrics to be collected and exported through Ditto's metrics pipeline.  The integration acts as a bridge, converting metrics-rs metrics into Ditto's internal format while preserving labels, units, and other metadata.  Possible values: "true", "false" Default: "false"  NOTE: Runtime changes of this SystemParameter are ignored. The integration must be configured at startup.

### metrics_exporter_metrics_rs_proxy_enabled
- **Type:** `bool`
- **Default value:** false
- **Bounds:** No additional restrictions on the value
- **Remarks:** Enables or disables the Proxy metrics exporter.  When enabled, Ditto will forward all metrics to an externally initialized metrics-rs exporter.  Possible values: "true", "false" Default: "false"  NOTE: Runtime changes of this SystemParameter are ignored.

### metrics_exporter_metrics_rs_proxy_level
- **Type:** `alloc::string::String`
- **Default value:** "info"
- **Bounds:** No additional restrictions on the value
- **Remarks:** System parameter for configuring the minimum metric level to export.  Possible values: "trace", "debug", "info" Default: "info"

### metrics_exporter_onfile_enabled
- **Type:** `bool`
- **Default value:** false
- **Bounds:** No additional restrictions on the value
- **Remarks:** Enables or disables the on-file metrics exporter.  When enabled, Ditto will periodically export metrics to CSV files with automatic rotation based on size, age, and file count limits. This provides persistent metrics storage for offline analysis and monitoring.  Possible values: "true", "false" Default: "false"  NOTE: Runtime changes of this SystemParameter are ignored.

### metrics_exporter_onfile_garbage_collection_age_secs
- **Type:** `ditto_configuration::types::Seconds`
- **Default value:** Seconds(60)
- **Bounds:** No additional restrictions on the value
- **Remarks:** 

### metrics_exporter_onfile_garbage_collection_interval_secs
- **Type:** `ditto_configuration::types::Seconds`
- **Default value:** Seconds(30)
- **Bounds:** No additional restrictions on the value
- **Remarks:** 

### metrics_exporter_onfile_histogram_quantiles
- **Type:** `alloc::string::String`
- **Default value:** "0,0.5,0.9,0.95,0.99,1"
- **Bounds:** No additional restrictions on the value
- **Remarks:** The histogram quantiles to export when writing metrics to file.

### metrics_exporter_onfile_histogram_summary_alpha
- **Type:** `f64`
- **Default value:** 0.0001
- **Bounds:** No additional restrictions on the value
- **Remarks:** Alpha represents the desired relative error for the summary.  The default value is the same as `Summary::with_defaults()` See https://docs.rs/metrics-util/latest/metrics_util/storage/struct.Summary.html.  NOTE: Runtime changes of this SystemParameter are ignored.

### metrics_exporter_onfile_histogram_summary_max_buckets
- **Type:** `u32`
- **Default value:** 32768
- **Bounds:** No additional restrictions on the value
- **Remarks:** Max_buckets controls how many subbuckets are created, which directly influences memory usage. Each bucket "costs" eight bytes, so a summary with 2048 buckets would consume a maximum of around 16 KiB.  The default value is the same as `Summary::with_defaults()` See https://docs.rs/metrics-util/latest/metrics_util/storage/struct.Summary.html.  NOTE: Runtime changes of this SystemParameter are ignored.

### metrics_exporter_onfile_histogram_summary_min_value
- **Type:** `f64`
- **Default value:** 1e-9
- **Bounds:** No additional restrictions on the value
- **Remarks:** Min value controls the smallest value that will be recognized distinctly from zero. Said in another way, any value between `-min_value` and `min_value` will be counted as zero.  The default value is the same as `Summary::with_defaults()` See https://docs.rs/metrics-util/latest/metrics_util/storage/struct.Summary.html.  NOTE: Runtime changes of this SystemParameter are ignored.

### metrics_exporter_onfile_interval_secs
- **Type:** `ditto_configuration::types::Seconds`
- **Default value:** Seconds(30)
- **Bounds:** No additional restrictions on the value
- **Remarks:** The interval (in seconds) at which the metrics exporter writes to file.

### metrics_exporter_onfile_level
- **Type:** `alloc::string::String`
- **Default value:** "info"
- **Bounds:** No additional restrictions on the value
- **Remarks:** The metrics level to export to file.

### metrics_exporter_onfile_max_file_age_secs
- **Type:** `ditto_configuration::types::Seconds`
- **Default value:** Seconds(86400)
- **Bounds:** No additional restrictions on the value
- **Remarks:** Maximum age of a log file before rotation (in seconds).  NOTE: Runtime changes of this SystemParameter are ignored.

### metrics_exporter_onfile_max_file_size
- **Type:** `ditto_configuration::types::Bytes`
- **Default value:** Bytes(10485760)
- **Bounds:** No additional restrictions on the value
- **Remarks:** Maximum size of a single log file before rotation (in bytes).  NOTE: Runtime changes of this SystemParameter are ignored.

### metrics_exporter_onfile_max_files
- **Type:** `usize`
- **Default value:** 18446744073709551615
- **Bounds:** No additional restrictions on the value
- **Remarks:** Maximum number of old log files to keep.  NOTE: Runtime changes of this SystemParameter are ignored.

### metrics_exporter_onfile_overwrite
- **Type:** `bool`
- **Default value:** true
- **Bounds:** No additional restrictions on the value
- **Remarks:** Whether or not to overwrite existing metrics files. NOTE: Runtime changes of this SystemParameter are ignored.

### metrics_exporter_prometheus_enabled
- **Type:** `bool`
- **Default value:** false
- **Bounds:** No additional restrictions on the value
- **Remarks:** Enables or disables the Prometheus metrics exporter.  When enabled, Ditto will expose metrics via an HTTP endpoint that can be scraped by Prometheus servers. The exporter translates Ditto's internal metrics format to Prometheus-compatible format.  Possible values: "true", "false" Default: "false"  NOTE: Runtime changes of this SystemParameter are ignored.

### metrics_exporter_prometheus_garbage_collection_age_secs
- **Type:** `ditto_configuration::types::Seconds`
- **Default value:** Seconds(60)
- **Bounds:** No additional restrictions on the value
- **Remarks:** Garbage collection age controls how old metrics can be before they are removed from the ditto-metrics/metrics-rs translation layer.  WARNING: garbage collection only applies to the ditto-metrics/metrics-rs translation layer, NOT to the metrics-rs Prometheus exporter. Once exported, metrics remain in Prometheus indefinitely. This is a limitation of the metrics-rs Prometheus exporter. See `METRICS_EXPORTER_PROMETHEUS_LEVEL`.

### metrics_exporter_prometheus_garbage_collection_interval_secs
- **Type:** `ditto_configuration::types::Seconds`
- **Default value:** Seconds(30)
- **Bounds:** No additional restrictions on the value
- **Remarks:** Garbage collection interval controls how often stale metrics are removed from the ditto-metrics/metrics-rs translation layer. Metrics older than METRICS_EXPORTER_PROMETHEUS_GARBAGE_COLLECTION_AGE_SECS are removed.  WARNING: garbage collection only applies to the ditto-metrics/metrics-rs translation layer, NOT to the metrics-rs Prometheus exporter. Once exported, metrics remain in Prometheus indefinitely. This is a limitation of the metrics-rs Prometheus exporter. See `METRICS_EXPORTER_PROMETHEUS_LEVEL`.

### metrics_exporter_prometheus_http_listener_addr
- **Type:** `alloc::string::String`
- **Default value:** "0.0.0.0:9000"
- **Bounds:** No additional restrictions on the value
- **Remarks:** System parameter for configuring the HTTP listener address.  The address where Prometheus can scrape metrics from. Default: "0.0.0.0:9000"  NOTE: Runtime changes of this SystemParameter are ignored.

### metrics_exporter_prometheus_level
- **Type:** `alloc::string::String`
- **Default value:** "info"
- **Bounds:** No additional restrictions on the value
- **Remarks:** System parameter for configuring the minimum metric level to export to Prometheus.  Possible values: "trace", "debug", "info" Default: "info"  NOTE: due to ditto-metrics' hierarchical aggregation (fine-grained metrics with dynamic labels roll up into top-level metrics), the translation layer tracks ALL metrics internally regardless of this setting to maintain correct metrics aggregation.  WARNING: garbage collection only applies to the ditto-metrics/metrics-rs translation layer, NOT to the metrics-rs Prometheus exporter. Once exported, metrics remain in Prometheus indefinitely. This is a limitation of the metrics-rs Prometheus exporter. Setting this to "debug" or "trace" significantly increases memory consumption and cardinality in Prometheus.  Use cautiously in production.

### metrics_exporter_prometheus_mode
- **Type:** `alloc::string::String`
- **Default value:** "http-listener"
- **Bounds:** No additional restrictions on the value
- **Remarks:** Prometheus exporter mode configuration.  Determines how the Prometheus exporter operates: - `"http-listener"` (default): Starts an HTTP server on the configured address for Prometheus scraping - `"handle"`: Exposes metrics through a handle without starting an HTTP server

### metrics_exporter_virtual_collection_enabled
- **Type:** `bool`
- **Default value:** false
- **Bounds:** No additional restrictions on the value
- **Remarks:** Enables or disables the virtual collection metrics exporter.  When enabled, Ditto will expose metrics through a queryable virtual collection accessible via DQL. This allows remote queries to retrieve real-time metrics data including counters, gauges, and histograms with their associated metadata.  Possible values: `true`, `false` Default: `false`  **Note**: Runtime changes of this system parameter are ignored. The exporter must be enabled at startup.

### metrics_exporter_virtual_collection_garbage_collection_age_secs
- **Type:** `ditto_configuration::types::Seconds`
- **Default value:** Seconds(60)
- **Bounds:** No additional restrictions on the value
- **Remarks:** 

### metrics_exporter_virtual_collection_garbage_collection_interval_secs
- **Type:** `ditto_configuration::types::Seconds`
- **Default value:** Seconds(30)
- **Bounds:** No additional restrictions on the value
- **Remarks:** 

### metrics_exporter_virtual_collection_histogram_quantiles
- **Type:** `alloc::string::String`
- **Default value:** "0,0.5,0.9,0.95,0.99,1"
- **Bounds:** No additional restrictions on the value
- **Remarks:** The histogram quantiles to export when answering a remote query.  This parameter specifies which quantile values to calculate and include when exporting histogram metrics. Quantiles provide percentile information about the distribution of values (e.g., p50/median, p95, p99).  The value should be a comma-separated list of decimal numbers between 0 and 1.  Default: `"0,0.5,0.9,0.95,0.99,1"` (min, median, p90, p95, p99, max)

### metrics_exporter_virtual_collection_level
- **Type:** `alloc::string::String`
- **Default value:** "info"
- **Bounds:** No additional restrictions on the value
- **Remarks:** The minimum metrics level to export when answering a remote query.  This parameter filters which metrics are included in query results based on their importance level. Only metrics at or above the specified level will be exported.  Possible values: `"info"`, `"debug"`, `"trace"` Default: `"info"`

### monotonic_sender_queue_size
- **Type:** `u64`
- **Default value:** 32
- **Bounds:** At least 1 (inclusive)
- **Remarks:** The size of this channel is important _w.r.t._ backpressure when the events (because of the documents, see <https://github.com/getditto/ditto/issues/1755>) become huge (say, 5MiB or even more!).  Otherwise we run into issues such as the whole discussion that preceded <https://dittolive.slack.com/archives/CJCFGTK9T/p1639582317230000>  See <https://github.com/getditto/ditto/pull/4737> for more info.  Note that this is read once when [`crate::SmallPeerStore`] is created; updates to this parameter on a running system will not be reflected for [`crate::SmallPeerStore`]s that already exist until the peer is restarted.

### multicast_discovery_interval_secs
- **Type:** `ditto_configuration::types::Milliseconds`
- **Default value:** Milliseconds(3000)
- **Bounds:** No additional restrictions on the value
- **Remarks:** Frequency at which to send multicast discovery messages. A higher frequency means faster discovery at the cost of increased network traffic.

### multicast_discovery_port
- **Type:** `u64`
- **Default value:** 5000
- **Bounds:** At least 0 (inclusive) and at most 65535 (inclusive)
- **Remarks:** Port on the multicast IP to which MulticastMessages are sent. All peers must use the same port for this discovery method to work!

### network_enable_ngn
- **Type:** `bool`
- **Default value:** false
- **Bounds:** No additional restrictions on the value
- **Remarks:** Switch to enable/disable next-generation networking. This parameter should only be used in coordination with the networking/transport teams and is not intended for general use.  Disabling this parameter _may_ reduce cpu usage but network traffic should be unaffected. Ditto Data Streams / Bus will not work correctly if this is disabled. The primary use case for this flag is to aid in debugging Ditto Data Streams applications, preventing traffic being routed through this device.  This is always disabled on the Big Peer at this time.  Note: This parameter only takes effect when set at startup. Changing it at runtime will have no effect. The recommended way to set this parameter is via its associated environment variable.

### network_routing_algorithm
- **Type:** `u64`
- **Default value:** 2
- **Bounds:** At least 0 (inclusive) and at most 3 (inclusive)
- **Remarks:** EXPERIMENTAL: A quick and dirty system parameter to allow us to set the required router type for the next gen IP router stack at system startup. The mapping of integer -> routing algorithm is as follows:  0 - Never a Next Hop (only applies to legacy) 1 - Destination is Next Hop (only applies to legacy) 2 - REMOVED (was OSPF): will fall back Babel if available. 3 - Babel (requires experimental-bus feature) 4..=u64::MAX - Reserved

### object_store_fs_read_buffer_capacity
- **Type:** `u64`
- **Default value:** 8192
- **Bounds:** At least 1 (inclusive) and at most 4294967295 (inclusive)
- **Remarks:** Controls the capacity of the object store read buffer.  The default is the same value chosen by the standard library for [`StdBufReader`].  This is important for limiting the number of OS calls made by an object reader. Without it, there is a pathological worst case where many small reads could induce extreme overhead.

### object_store_s3_client_request_retry_initial_backoff_ms
- **Type:** `ditto_configuration::types::Milliseconds`
- **Default value:** Milliseconds(1000)
- **Bounds:** At least 1 (inclusive)
- **Remarks:** Initial backoff when an HTTP request to S3 is being retried.  This is a parameter used by the exponential backoff retry strategy. It does _NOT_ mean that the SDK will actually wait for this amount of time before performing the first retry. See [`RetryConfig::with_initial_backoff`] for more details.

### object_store_s3_client_request_retry_max_attempts
- **Type:** `u64`
- **Default value:** 2
- **Bounds:** At least 1 (inclusive) and at most 4294967295 (not including 4294967295)
- **Remarks:** The maximum number of attempts made when calling an S3 API. There will be multiple attempts if a request has timed out or has failed and is retryable.

### object_store_s3_client_request_retry_max_backoff_ms
- **Type:** `ditto_configuration::types::Milliseconds`
- **Default value:** Milliseconds(5000)
- **Bounds:** At least 1 (inclusive)
- **Remarks:** Maximum backoff when an HTTP request to S3 is being retried.  This is a parameter used by the exponential backoff retry strategy. This sets an upper bound on time before the next retry. See [`RetryConfig::with_max_backoff`] for more details.

### object_store_s3_client_request_timeout_ms
- **Type:** `ditto_configuration::types::Milliseconds`
- **Default value:** Milliseconds(5000)
- **Bounds:** At least 1 (inclusive)
- **Remarks:** Timeout for an HTTP request to S3.

### object_store_s3_multipart_upload_intermediate_part_size
- **Type:** `u64`
- **Default value:** 5242880
- **Bounds:** At least 5242880 (inclusive) and at most 5368709120 (inclusive)
- **Remarks:** This parameter controls the amount of data buffered in-memory while an object is being written to S3. If this amount is reached, the object store dispatches an [UploadPart](https://docs.aws.amazon.com/AmazonS3/latest/API/API_UploadPart.html) request.  [Based on the documentation](https://docs.aws.amazon.com/AmazonS3/latest/userguide/qfacts.html), this parameter has been constrained to [5MiB, 5Gib].

### presence_debounce_interval
- **Type:** `ditto_configuration::types::Milliseconds`
- **Default value:** Milliseconds(500)
- **Bounds:** No additional restrictions on the value
- **Remarks:** A recommended small debounce interval applied to all multihop changes (even those requiring high frequency updates).  We must balance the speed of convergence gained by immediate link state broadcasts with bandwidth saturation and unnecessary work for rapid link changes. Of particular importance to debounce are: - Fast `PeerEvent::ConnectionEstablished` -> `PeerEvent::VirtConnJoined` -> `PeerEvent::VirtConnElectedNewPhy` transitions. - Cascading `PeerEvent::TransportOffline` -> `PeerEvent::VirtConnElectedNewPhy(None)` -> `PeerEvent::ConnectionEnded` events. - Multiple, simultaneous connection flows to different peers on joining a new mesh.

### presence_keep_alive_interval
- **Type:** `ditto_configuration::types::Seconds`
- **Default value:** Seconds(30)
- **Bounds:** No additional restrictions on the value
- **Remarks:** The maximum interval which can pass before a presence update is persisted and replicated to other peers. If this keepalive interval is hit without any link state changes occurring in the interim, we post a simple timestamp bump update - our last multihop update with the timestamp field set to `now()`.

### presence_multihop_update_ttl
- **Type:** `ditto_configuration::types::Seconds`
- **Default value:** Seconds(60)
- **Bounds:** No additional restrictions on the value
- **Remarks:** Duration after which data received via multihop updates is considered stale and will be ignored.  Note that no additional buffer or (multiplication factor higher than 2) is required since we're not dealing with a lossy transport mechanism. The 2x factor is relatively arbitrary.

### presence_query_recreation_interval
- **Type:** `ditto_configuration::types::Seconds`
- **Default value:** Seconds(3600)
- **Bounds:** No additional restrictions on the value
- **Remarks:** Re-create our live-query every 1hr. This is a balancing act. We don't want to recreate our query too often as doing so would invalidate some replication optimizations (`from` queries, etc). Not renewing this often enough, however, will cause us to receive obsolete presence payloads much older than our TTL.

### presence_use_multihop
- **Type:** `bool`
- **Default value:** true
- **Bounds:** No additional restrictions on the value
- **Remarks:** Switch to enable multihop presence in the mesh subsystem. When enabled (default) the presence subsystem will write it's connectivity information to a `__presence` collection that is replicated between peers as a from of link state announcement topology information.  Disabling this parameter will reduce network traffic, especially in larger more dynamic meshes as we do not need to replicate presence information between peers.  Note: Multihop networking capabilities are currently built atop this information so disabling multihop presence will result in those features not functioning as expected.  Note: This parameter only takes effect when set at startup. Changing it at runtime will have no effect. The recommended way to set this parameter is via its associated environment variable.  todo(frankie.foston) - Once we have reworked the ChannelRepo we can make this runtime configurable

### reaper_preferred_hour
- **Type:** `u64`
- **Default value:** 0
- **Bounds:** At least 0 (inclusive) and at most 24 (not including 24)
- **Remarks:** Tombstone reaping is an infrequent background task concerned with automatic cleanup of tombstones that have expired their TTLs.  This lets us set the preferred hour of the day to run tombstone reaping in the local timezone using 24-hour time.  This value is only used if [`ENABLE_REAPER_PREFERRED_HOUR_SCHEDULING`] is set. See the documentation for that setting for more detail on the behavior when using preferred-hour scheduling for tombstone reaping.  As mentioned in the docs for [`ENABLE_REAPER_PREFERRED_HOUR_SCHEDULING`], preferred-hour scheduling is not supported under WASM.

### reaper_timing_random_variation_seconds
- **Type:** `u64`
- **Default value:** 600
- **Bounds:** At least 0 (inclusive) and at most 3000 (not including 3000)
- **Remarks:** Tombstone reaping is an infrequent background task concerned with automatic cleanup of tombstones that have expired their TTLs.  In order to prevent mesh-wide load spikes due to many peers triggering the reaper simultaneously, we include an additional randomized delay when calculating the scheduled reaping times. If this setting is set to `0`, then no such delay will be added, otherwise we delay by an amount between zero and the number of seconds specified in this config parameter.

### remote_flamegraph_request_enabled
- **Type:** `bool`
- **Default value:** false
- **Bounds:** No additional restrictions on the value
- **Remarks:** Remote flamegraph requests are an experimental tool, intended to help in debugging.  Enable the feature through this parameter, and make requests via the portal's device dashboard.

### remote_query_request_timeout_secs
- **Type:** `ditto_configuration::types::Seconds`
- **Default value:** Seconds(90)
- **Bounds:** No additional restrictions on the value
- **Remarks:** The amount of time to wait before giving up on a remote query request, in seconds.  If this time elapses without getting a response back, we'll return an error back to the client.

### remote_query_response_size_limit_bytes
- **Type:** `u64`
- **Default value:** 10485760
- **Bounds:** No additional restrictions on the value
- **Remarks:** The maximum allowed size of the response value in the remote query response message, in bytes.  If a reply to a remote query request would exceed this limit, an error is returned to the client.

### replication_change_listener_event_queue_capacity
- **Type:** `u64`
- **Default value:** 64
- **Bounds:** At least 1 (inclusive)
- **Remarks:** The maximum number of [`CommitEvent`]s to be held into the [`floorcast`] queue before _subscribers_ start getting [`floorcast::error::RecvError::Lagged`] errors.  The `Lagged` error delivers the first in the stream of lagged `CommitEvents`, allowing the subscriber to fall back to [`AffectedDocs::Unknown`] handling semantics, while still being able to use the event timestamp for response timing purposes.  This parameter can only be tuned via an environment variable, and does not support changes at runtime.

### replication_change_listener_max_doc_keys
- **Type:** `u64`
- **Default value:** 1024
- **Bounds:** At least 1 (inclusive)
- **Remarks:** The maximum number of `DocumentKey`s to keep track of before falling back to [`AffectedDocs::Unknown`].

### replication_gc_apps_fraction
- **Type:** `f64`
- **Default value:** 0.04
- **Bounds:** No additional restrictions on the value
- **Remarks:** Background replication in a multi-app mode will collect only a fraction of all apps on each sweep. By default, we collect ~1/24th of the total number of apps in each ~hourly sweep. (0.04 =~ 1/24).

### replication_gc_base_interval_secs
- **Type:** `u64`
- **Default value:** 2700
- **Bounds:** No additional restrictions on the value
- **Remarks:** Replication background GC runs at a fixed interval, with jitter added to avoid periodic interference (see [`REPLICATION_GC_MAX_JITTER_SECS`]). This parameter controls the base interval, i.e. the number of seconds to which a random jitter is added to calculate the final value for how long to wait before the next GC sweep.

### replication_gc_max_jitter_secs
- **Type:** `u64`
- **Default value:** 1800
- **Bounds:** No additional restrictions on the value
- **Remarks:** We apply a random jitter on top of the base interval ([`REPLICATION_GC_BASE_INTERVAL_SECS`]) when scheduling background GC to help avoid a buildup of common cleanup/janitor tasks throughout the whole ditto system at common time boundaries (i.e. a period every 10 minutes attachments link maintainer running at the same time as an hourly replication GC). The jitter is very large (0..30 minutes) so that we maintain high probability of a good spread amongst several subscription servers in a Big Peer context.

### replication_gc_startup_delay_secs
- **Type:** `u64`
- **Default value:** 20
- **Bounds:** No additional restrictions on the value
- **Remarks:** We delay the first background GC by a small amount to ensure that we don't have too many processes competing for previous CPU/disk resources at the same time as the host app might be racing to show the first data on screen.

### replication_gc_ttl_secs
- **Type:** `u64`
- **Default value:** 604800
- **Bounds:** No additional restrictions on the value
- **Remarks:** Background replication GC will remove all remote peer data for peers which haven't been seen in ~7 days.

### replication_metadata_max_invalidated_ids
- **Type:** `u64`
- **Default value:** 1024
- **Bounds:** At least 1 (inclusive)
- **Remarks:** The maximum number of invalidated document keys to track before a database rescan is triggered.  This limit exists in order to guard against unbounded memory usage caused by tracking a theoretically unlimited number of invalidated IDs, which are a special case that need to be separately queried. When the limit is reached, the invalidated IDs are cleared and a database rescan is performed.

### replication_over_ngn
- **Type:** `bool`
- **Default value:** false
- **Bounds:** No additional restrictions on the value
- **Remarks:** Controls whether to use Next-Gen Networking (NGN) Data Streams for replication instead of legacy channel-based replication. When enabled, the system will register an NGN replication service that uses Data Streams over QUIC for document and attachment synchronization. When disabled (default), the system uses the existing channel-based replication service.  **Important**: All peers in the network must use the same setting. Mixed networks with some peers using NGN and others using legacy channels are not supported and will not be able to replicate data between each other.  This parameter only takes effect when set at startup. Changing it at runtime will have no effect. The recommended way to set this parameter is via its associated environment variable.  Default: `false` (use legacy channel-based replication for backward compatibility)

### replication_session_request_timeout_secs
- **Type:** `ditto_configuration::types::Seconds`
- **Default value:** Seconds(120)
- **Bounds:** No additional restrictions on the value
- **Remarks:** Maximum time allowed for a session to either handle a message or generate the next message. This is an arbitrarily high timeout that should never be reached during normal operation. If it _is_ reached then we should abort so that the Channel can be dropped and the VC can shut down. Motivating issues: * #6626 - suspected replication machine jam, never proven * #7547 - implementing a deliberate jam to avoid crash when handling an overlarge query

### replication_throttle_outbound_updates
- **Type:** `bool`
- **Default value:** false
- **Bounds:** No additional restrictions on the value
- **Remarks:** When set to `true`, limits the rate of outbound replication messages produced when in the syncing state to no more frequently than the value of [`replication_throttled_outbound_updates_delay_ms`][delay].  [delay]: THROTTLED_OUTBOUND_UPDATES_DELAY_MS

### replication_throttled_outbound_updates_delay_ms
- **Type:** `ditto_configuration::types::Milliseconds`
- **Default value:** Milliseconds(250)
- **Bounds:** No additional restrictions on the value
- **Remarks:** When the [`replication_throttle_outbound_updates`][toggle] parameter is set to `true`, outbound replication messages produced when in the syncing state will be limited to no more frequently than the value of this parameter.  [toggle]: THROTTLE_OUTBOUND_UPDATES

### rotating_log_file_max_age_h
- **Type:** `ditto_configuration::types::Hours`
- **Default value:** Hours(24)
- **Bounds:** At least 1 (inclusive)
- **Remarks:** The maximum age of each rotating on-disk log file before it's rotated away from.  This limit applies alongside `ditto_rotating_log_file_max_size_mb`, and files will be rotated away from when they hit whichever of the two limits applies first. For example, with the default values of these two parameters, a log file will be rotated away from if it reaches an estimated 1 MB in size in under 24 hours; otherwise, it will be rotated away from 24 hours after it was first written to (regardless of its size).  The maximum possible time range covered by the on-disk logger can be calculated using a combination of the value of this parameter and `ditto_rotating_log_file_max_files_on_disk`. For example (again using the default parameter values), a maximum of 15 files containing no more than 24 hours of log data results in a maximum time range of 15 days. In practice, this maximum time interval will only be fully covered if the volume of logs emitted during that time is sufficiently low as to not cause log rotation due to the size limit before the maximum age of each file is reached.  Currently has no effect on Wasm-based platforms.

### rotating_log_file_max_files_on_disk
- **Type:** `u64`
- **Default value:** 15
- **Bounds:** At least 3 (inclusive) and at most 30 (inclusive)
- **Remarks:** The maximum number of rotating on-disk log files that will be kept before the oldest is deleted.  The maximum possible disk usage of the on-disk logger can be calculated using a combination of the value of this parameter and `ditto_rotating_log_file_max_size_mb`. For example, using the default values of these two parameters, a maximum of 15 files containing no more than 1 MB of data each results in a maximum disk usage of 15 MB. In practice, disk usage might be slightly higher at any given time due to the currently active output file, which is uncompressed and might exceed this per-file limit until it is compressed.  The maximum possible time range covered by the on-disk logger can be calculated using a combination of the value of this parameter and `ditto_rotating_log_file_max_age_h`. For example, using the default parameter values, a maximum of 15 files containing no more than 24 hours of log data results in a maximum time range of 15 days. In practice, this maximum time interval will only be fully covered if the volume of logs emitted during that time is sufficiently low as to not cause log rotation due to the size limit before the maximum age of each file is reached.  Currently has no effect on Wasm-based platforms.

### rotating_log_file_max_size_mb
- **Type:** `ditto_configuration::types::Megabytes`
- **Default value:** Megabytes(1)
- **Bounds:** At least 1 (inclusive) and at most 10 (inclusive)
- **Remarks:** The target maximum size, in megabytes, of each compressed rotating on-disk log file. Since compressing log data significantly reduces its size, many times more data than this will be written to the active log file at any given time - but when rotating away from the file, it will be compressed, reducing its size significantly.  This limit applies alongside `ditto_rotating_log_file_max_age_h`, and files will be rotated away from when they hit whichever of the two limits applies first. For example, with the default values of these two parameters, a log file will be rotated away from if it reaches an estimated 1 MB in compressed size in under 24 hours; otherwise, it will be rotated away from 24 hours after it was first written to (regardless of its size).  The maximum possible disk usage of the on-disk logger's compressed files can be calculated using a combination of the value of this and `ditto_rotating_log_file_max_files_on_disk`. For example (again using the default parameter values), a maximum of 15 files containing no more than 1 MB of data results in a maximum disk usage of 15 MB. In practice, disk usage might be slightly higher at any given time due to the currently active output file, which is uncompressed and might exceed this per-file limit until it is compressed.  Currently has no effect on Wasm-based platforms.

### s3_blobstore_cache_capacity
- **Type:** `u32`
- **Default value:** 10
- **Bounds:** At least 1 (inclusive) and at most 4294967295 (inclusive)
- **Remarks:** Controls how many blobs will be held in memory as a cache for get/head operations.

### selective_eviction_filter_ids_per_query
- **Type:** `u64`
- **Default value:** 5000
- **Bounds:** At least 1 (inclusive) and at most 4294967295 (inclusive)
- **Remarks:** Controls the maximum number of document summaries that will be queried at once when generating an eviction filter, when synchronizing after an eviction at the local peer.  Decreasing this limit may decrease memory allocations when generating eviction filters for sessions that are tracking more documents, but at the cost of more queries to the document store, which may cause higher update latency.  This parameter is ignored if [`ENABLE_SELECTIVE_EVICTION_FILTERS`] is not enabled.

### small_peer_info_collection_stats_include_internal
- **Type:** `bool`
- **Default value:** false
- **Bounds:** No additional restrictions on the value
- **Remarks:** Whether to publish "internal" collections' stats.  Small peer info is a replicated collection. Each small peer publishes a single document into this collection, and the big peer uses them to present the device dashboard interface.  One of the things that's published is statistics about every collection available locally on the current peer. This flag controls whether stats about "internal" collections (created and used internally by Ditto, not by customer usage) are included. The default is false.  Enabling this is only likely to be useful when debugging hand-in-hand with Ditto support and engineering.

### small_peer_info_collection_stats_limit
- **Type:** `u64`
- **Default value:** 32
- **Bounds:** No additional restrictions on the value
- **Remarks:** Maximum number of collections' stats to publish.  Small peer info is a replicated collection. Each small peer publishes a single document into this collection, and the big peer uses them to present the device dashboard interface.  One of the things that's published is statistics about every collection available locally on the current peer. Because this is a (potentially) unbounded set, we have this artificial upper limit on the number of collections' worth of stats that are published. The default is 32.  The tradeoff, here, is between the cost of replicating small peer info itself, and observability in cases where there are an unexpectedly large number of local collections.

### small_peer_info_is_enabled
- **Type:** `bool`
- **Default value:** true
- **Bounds:** No additional restrictions on the value
- **Remarks:** Whether this small peer's info document should be published.  Small peer info is a replicated collection. Each small peer publishes a single document into this collection, and the big peer uses them to present the device dashboard interface.  Disabling this feature means that no new small peer info publishing will happen, and therefore that this small peer won't appear in the device dashboard (or will appear, but with stale information.)

### small_peer_info_local_subscriptions_limit
- **Type:** `u64`
- **Default value:** 16
- **Bounds:** No additional restrictions on the value
- **Remarks:** Maximum number of local subscription queries to publish.  Small peer info is a replicated collection. Each small peer publishes a single document into this collection, and the big peer uses them to present the device dashboard interface.  One of the things that's published is the list of currently-active local subscription queries. Because this is a (potentially) unbounded set, we have this artificial upper limit on the number of queries that are published. The default is 16.  The tradeoff, here, is between the cost of replicating small peer info itself, and visibility of local subscriptions in cases where there are an unexpected number of them.

### small_peer_info_publish_interval_secs
- **Type:** `ditto_configuration::types::Seconds`
- **Default value:** Seconds(300)
- **Bounds:** At least 1 (inclusive)
- **Remarks:** The interval, in seconds, at which this small peer's info document is written.  Small peer info is a replicated collection. Each small peer publishes a single document into this collection, and the big peer uses them to present the device dashboard interface.  A background task is responsible for periodically publishing the relevant information. The default is every 5 minutes.  It's really important to note that this *publishing* frequency is decoupled from the *collection* frequency! Each piece of small peer info is backed by internal data sources with their own collection logic, and tuning this parameter won't change the granularity of collection—only the frequency with which the data are published for replication!  Therefore, running the small peer info publishing task more frequently increases the *best case* time granularity of small peer info that'll be visible on the device dashboard, at the cost of increased replication traffic.  Running the task less frequently, conversely, decreases the *best case* time granularity of small peer info, but reduces replication traffic.

### sqlite3_begin_txn_timeout_secs
- **Type:** `ditto_configuration::types::Seconds`
- **Default value:** Seconds(60)
- **Bounds:** No additional restrictions on the value
- **Remarks:** Timeout for internal blocking when starting a new transaction.  Every database operation in Ditto is backed by a transaction. Starting a transaction can be blocked by several factors, including: the connection pool being full, the worker pool being full, or the existence of another concurrent write transaction.  This parameter sets the maximum time Ditto will spend trying to start a new transaction, before it is returned as an error at the point where the transaction was created.

### sqlite3_busy_timeout_secs
- **Type:** `ditto_configuration::types::Seconds`
- **Default value:** Seconds(60)
- **Bounds:** At least 1 (inclusive) and at most 2147483 (inclusive)
- **Remarks:** Timeout for internal blocking when waiting for a locked database resource.  When a database operation is attempted on a resource that's locked by another task, Ditto will retry several times before returning a failure.  This parameter sets the maximum time Ditto will spend trying obtain a lock, before it is returned as an error at the point where the database operation was attempted.  Note that updates to this parameter made at runtime on a live peer will be reflected in new database connections, but existing connections will continue to use the same timeout that they saw when they were created.

### sqlite3_connection_timeout_secs
- **Type:** `ditto_configuration::types::Seconds`
- **Default value:** Seconds(30)
- **Bounds:** At least 1 (inclusive)
- **Remarks:** Timeout for internal blocking when adding a new database connection to the internal pool.  As an optimization, Ditto caches and reuses a pool of database connections. This is an internal detail that isn't exposed in the SDK, but there may be edge cases where tuning it is necessary.  This parameter sets the maximum interval Ditto will spend attempting to add a new connection to the internal pool.  Note that this value is only used once, on Ditto startup. Changing it after Ditto has already started has no effect.

### sqlite3_env_drop_timeout_secs
- **Type:** `ditto_configuration::types::Seconds`
- **Default value:** Seconds(60)
- **Bounds:** No additional restrictions on the value
- **Remarks:** Time to wait for all connections of a Backend to go away on Backend Drop.  When Ditto shuts down gracefully, any in-progress database operations are notified, and then given time to complete.  This parameter sets an upper bound on the time to wait for in-progress database operations to complete, during a graceful shutdown. If the work takes longer than this, Ditto will escalate by crashing.

### sqlite3_max_connections
- **Type:** `u64`
- **Default value:** 60
- **Bounds:** At least 32 (inclusive) and at most 4294967295 (inclusive)
- **Remarks:** Maximum number of database connections in the internal pool.  As an optimization, Ditto caches and reuses a pool of database connections. This is an internal detail that isn't exposed in the SDK, but there may be edge cases where tuning it is necessary.  This parameter sets an upper bound on the size of the internal connection pool. If the demand for the database on a peer exceeds this upper bound, database operations will become slower (as they're enqueued waiting for an available connection) and eventually time out.  Note that this value is only used once, on Ditto startup. Changing it after Ditto has already started has no effect.

### sqlite3_workers_ttl_secs
- **Type:** `ditto_configuration::types::Seconds`
- **Default value:** Seconds(5)
- **Bounds:** No additional restrictions on the value
- **Remarks:** TTL for workers in the worker thread pool.  As an optimization, Ditto manages a pool of worker threads to perform database I/O. This is an internal detail that isn't exposed in the SDK, but there may be edge cases where tuning it is necessary.  This parameter sets how long an idle worker thread in this pool is kept around for possible reuse.  Note that this value is only used once, on Ditto startup. Changing it after Ditto has already started has no effect.

### store_observers_num_threads
- **Type:** `usize`
- **Default value:** 0
- **Bounds:** No additional restrictions on the value
- **Remarks:** Sets the number of threads used to dispatch store observer callback invocations.  `0` is used for a special meaning instead: "num_cpus", ie., one thread per core on the machine.  This is the default setting.  ---  For context, Ditto exposes reactive APIs to _observe_ changes to its local database (its `store`). Mainly, the `ditto.store.registerObserver()` API (or the legacy "live query" API: `ditto.collection(…).find…().observeLocal(…)`).  Such APIs take callbacks, which are then invoked by Ditto's own runtime.  This setting adjusts the amount of parallelism used when doing so.  For instance, when set to `1`, then these callback invocations happen *serially*, but that also means that if a single observer callback hogs the thread whence it is called, no other callback can run in the meantime.  No matter how parallel these callback invocations may be in general, for a given store observer, on the other hand, it is guaranteed that the code up until `signalNext()` is called does run serially (albeit potentially on a different thread each time). When not using `signalNext` explicitly (ie., with `LiveQueryAvailability::Always`), then the whole (synchronous) body of the callback is encompassed by this serial guarantee.  ---  Note: This setting has no effect on WebAssembly-based SDKs, including the Flutter and JavaScript SDKs when used in web browsers.

### system_info_max_folder_elements
- **Type:** `u64`
- **Default value:** 10
- **Bounds:** At least 1 (inclusive)
- **Remarks:** The maximum number of values that can be stored in a single system info folder.  The system info table is an internal table that stores some operational and diagnostic data.  A system info folder is a (namespace, key) pair that uniquely identifies a sparse time-series of related values.  This parameter sets the maximum number of values that can be stored in each folder, before the oldest values start being garbage collected. The default is 10.  Garbage collection is performed in the background by the vacuum task, whose frequency is controlled by "system_info_vacuum_interval_secs".  Increasing this threshold might help diagnose or debug infrequent issues, at the cost of a bit of extra local storage.  Decreasing this threshold might free up some local storage, at the cost of visibility of system info over time.

### system_info_max_transaction_batch_elements
- **Type:** `u64`
- **Default value:** 20
- **Bounds:** At least 1 (inclusive)
- **Remarks:** The maximum number of system info values that may be batched into a single write transaction.  The system info table is an internal table that stores some operational and diagnostic data.  The writer task is responsible for persisting values as soon as they're sent in. If more than one value is sent in at the same time, the writer may batch them into a single write transaction.  This parameter sets the maximum number of values the writer may batch together. The default is 20.  Increasing the batch size means fewer, larger write transactions when the system info table is under pressure.  Decreasing the batch size means more, smaller write transactions when the system info table is under pressure.

### system_info_store_worker_interval_secs
- **Type:** `u64`
- **Default value:** 600
- **Bounds:** At least 1 (inclusive)
- **Remarks:** The interval, in seconds, at which the store info worker task runs.  The system info table is an internal table that stores some operational and diagnostic data.  The store worker task is responsible for periodically gathering and publishing certain system info about the local key-value store—specifically, any information we don't track continuously, but instead need to recompute from the persistent store.  This parameter sets how often this task runs. The default is every 10 minutes.  Running the task more frequently means the system info will have more precise granularity, at the cost of more frequent full table scans.

### system_info_vacuum_interval_secs
- **Type:** `ditto_configuration::types::Seconds`
- **Default value:** Seconds(60)
- **Bounds:** At least 1 (inclusive)
- **Remarks:** The interval, in seconds, at which the system info vacuum task runs.  The system info table is an internal table that stores some operational and diagnostic data.  The vacuum task is responsible for garbage collection of the system info table: it periodically scans the entire table for data which no longer needs to be retained (which is determined by "system_info_max_folder_elements"!), and removes it.  This parameter sets how often the vacuum task runs. The default is every 60 seconds.  Running the task more frequently means excessive system info values are removed more quickly, at the cost of more frequent full table scans.  Running it less frequently allows excessive system info values to "leak" for longer, but reduces the number and frequency of full table scans.

### tcp_server_bind_mdns_server_port
- **Type:** `u64`
- **Default value:** 0
- **Bounds:** At least 0 (inclusive) and at most 65535 (inclusive)
- **Remarks:** TCP listening port used when peer-to-peer-sync on LAN is enabled.  Default value of "0" means an ephemeral (effectively random and unused) port will be selected and this should be preferred in most cases.  If your organization needs fine-grained control over firewall rules, then random port assignment for TCP connections between peers makes opening specific ports for Ditto difficult. You can use this system parameter to set the port that Ditto will bind to when hosting server connections for peer-to-peer TCP sync. By manually providing a port, this can be added to firewall rules to allow Ditto traffic.  This value must be set before calling `startSync()`. Unlike other system parameters, this system parameter does not live-update if changed after sync is already started.  If multiple apps using Ditto are running at the same time on the same device and configured to use the same port, then only one will succeed and the other apps will fail to sync due to the port already being in use.

### tiered_blob_store_max_blob_size_before_spillover_to_disk
- **Type:** `u64`
- **Default value:** 10000000
- **Bounds:** No additional restrictions on the value
- **Remarks:** When a blob exceeds this size, [`TieredBlobStore`] moves it from in-memory to on-disk.  This parameter is only relevant to subscription servers (Big Peer) which use this blob store to store document replication updates.  Increasing it will retain larger blobs in-memory (memory usage goes up, disk usage goes down). Decreasing it will store smaller blobs on-disk (memory usage goes down, disk usage goes up).

### tombstone_ttl_enabled
- **Type:** `bool`
- **Default value:** true
- **Bounds:** No additional restrictions on the value
- **Remarks:** Enables the automatic cleanup of tombstones that have expired their TTLs.  This is enabled by default, since the cleanup process is infrequent and should be inexpensive in most cases. If tombstones are never cleaned up, then they'll continue to use more and more space over time, which can be problematic. However, this flag can be safely disabled in use cases where tombstones are never written by any of the peers in the mesh.  Note that enabling this parameter on a running Ditto instance will *not* cause the peer to immediately check for expired tombstones; the peer will wait until the next time that we would normally execute this process (e.g. if the peer is configured to reap tombstones once a day around midnight, and this flag is set to `true` in the middle of the day, the peer will still wait until next time midnight rolls around, rather than immediately reaping tombstones when this parameter is enabled).

### tombstone_ttl_hours
- **Type:** `ditto_configuration::types::Hours`
- **Default value:** Hours(168)
- **Bounds:** At least 1 (inclusive) and at most 5124095576030 (not including 5124095576030)
- **Remarks:** The threshold age in hours after which a tombstone will be considered expired on the local peer.  Note that the timestamp used to determine the tombstone's age is written when the tombstone is created, so if a peer with an inaccurate clock creates a tombstone that is synced to this peer, the tombstone may be evicted from the local peer earlier or later than expected.  The default value of 168 hours is equal to one week.  Setting this to very low values may cause deletions to fail, because if all the devices in a mesh do not receive a tombstone before it is removed, the document may be "resurrected" by the peers that still have the document, as it will be synced back to devices that no longer have a corresponding tombstone. Of course, if all peers in a mesh are online, then even a single hour might easily be sufficient to sync the tombstones everywhere, but if even a single device is offline for an hour then a document resurrection could easily occur with such a low value.  Setting this to a very high value may cause perfomance problems, for two reasons: - More and more storage space may be consumed on the device over time if many deletions are done, as tombstones will linger and take up space for a long time. - The big peer's tombstone TTL may trigger before corresponding small peers, causing the tombstones to be sent back to the big peer after they've been evicted, leading to increased resource consumption as tombstones are repeatedly synced back to the big peer and evicted again.  Tombstones are quite small, though, and don't normally take up much storage space, so the conservative default of one week shouldn't pose any problems for typical use cases, unless a very high number of deletions are occurring on very space-constrained devices. If devices are normally expected to be offline for longer than a week, though, then a higher TTL value may be warranted.

### transaction_duration_before_logging_ms
- **Type:** `ditto_configuration::types::Milliseconds`
- **Default value:** Milliseconds(10000)
- **Bounds:** At least 1 (inclusive)
- **Remarks:** The amount of time that a transaction can be running for before logging — indicating that a long-running transaction is ongoing — should begin.  Specified as milliseconds instead of seconds in case we want to log sooner than after 1 second. The default is 10 seconds.  Attempting to set the time to 0 would trigger a panic, so this has a lower bound of 1.

### transaction_trace_interval_ms
- **Type:** `ditto_configuration::types::Milliseconds`
- **Default value:** Milliseconds(5000)
- **Bounds:** At least 1 (inclusive)
- **Remarks:** The interval of time between progressive trace statements if a DQL transaction is running for more than the configured time (specifically, the amount of time before logging should begin).  Specified as milliseconds instead of seconds in case we want to log more frequently than once a second. The default is 5 seconds.  Attempting to set the interval to 0 would trigger a panic, so this has a lower bound of 1.

### transport_tcp_client_forced_connection_max_delay
- **Type:** `ditto_configuration::types::Milliseconds`
- **Default value:** Milliseconds(30000)
- **Bounds:** No additional restrictions on the value
- **Remarks:** Maximum interval between TCP connection attempts for forced connections.

### transport_tcp_client_forced_connection_min_delay
- **Type:** `ditto_configuration::types::Milliseconds`
- **Default value:** Milliseconds(1000)
- **Bounds:** No additional restrictions on the value
- **Remarks:** Minimum interval between TCP connection attempts for forced connections.

### transports_awdl_browse_enabled
- **Type:** `bool`
- **Default value:** true
- **Bounds:** No additional restrictions on the value
- **Remarks:** When AWDL is enabled, controls whether Ditto will search for other peers.

### transports_awdl_registration_enabled
- **Type:** `bool`
- **Default value:** true
- **Bounds:** No additional restrictions on the value
- **Remarks:** When AWDL is enabled, controls whether Ditto will register (advertise) this peer.

### transports_ble_adapter_mac
- **Type:** `alloc::string::String`
- **Default value:** ""
- **Bounds:** No additional restrictions on the value
- **Remarks:** Specify MAC address to use for bluetooth on linux  If this parameter is left as a default empty string, the system will use the first available bluetooth adapter. If you have multiple bluetooth adapters and want to specify which one to use, set this parameter to the MAC address of the adapter you want to use i.e.: 01:02:03:04:05:06  Changes to this parameter take effect when the BLE transport is started.  This parameter is only used on linux systems.

### transports_ble_server_is_enabled
- **Type:** `bool`
- **Default value:** true
- **Bounds:** No additional restrictions on the value
- **Remarks:** Enable/Disable BLE Server Functionality  Disabling this parameter stops the BLE transport from advertising and accepting incoming connections from central devices.  This system parameter can be used in conjuction with `mesh_chooser_max_active_ble_clients` to deterministically control the total number of active BLE connections a device can have at a given time.

### transports_connection_failure_threshold
- **Type:** `u64`
- **Default value:** 3
- **Bounds:** No additional restrictions on the value
- **Remarks:** Number of failures within the window before logging at error level.  This threshold determines when connection failures should be escalated from warning level to error level logging. Once the number of failures to the same peer within the `CONNECTION_FAILURE_WINDOW` reaches or exceeds this threshold, subsequent failures are logged as errors instead of warnings.  # Purpose - Reduces noise in logs for expected transient failures - Highlights persistent connection problems that need attention - Improves log readability by using appropriate severity levels  # Default Value 3 failures  # Configuration Can be configured via system parameters using the key: `transports_connection_failure_threshold`  # Behavior - Failures 1 to (threshold-1): Logged at warning level - Failures >= threshold: Logged at error level  # Example With a threshold of 3 and a 60-second window: - 1st failure at 0s: Logged as warning - 2nd failure at 30s: Logged as warning - 3rd failure at 45s: Logged as ERROR (threshold reached) - 4th failure at 50s: Logged as ERROR - After 60s from the last failure, if no new failures occur, the counter resets

### transports_connection_failure_window_secs
- **Type:** `ditto_configuration::types::Seconds`
- **Default value:** Seconds(60)
- **Bounds:** No additional restrictions on the value
- **Remarks:** Time window for tracking connection failures to the same peer.  This parameter defines the sliding time window (in seconds) during which connection failures are tracked for each peer. When a connection failure occurs, it's recorded with a timestamp. Failures older than this window are automatically expired and removed from tracking.  # Purpose - Enables smart error level selection based on failure frequency  # Default Value 60 seconds (1 minute)  # Configuration Can be configured via system parameters using the key: `transports_connection_failure_window_secs`  # Example If set to 60 seconds and a peer fails to connect 5 times within a minute, all 5 failures are considered "recent". Failures from 61+ seconds ago are not counted toward the current failure rate.

### transports_discovered_peers
- **Type:** `alloc::vec::Vec<ditto_types::discovered_peer::DiscoveredPeer>`
- **Default value:** []
- **Bounds:** ArrayValidator with the following value validator:
	MapValidator with the following:
Key: address
	Required: true
	Value must satisfy: Must be a valid URL
Key: discovery_hint
	Required: false
	Value must satisfy: AnyValue
Key: type
	Required: false
	Value must satisfy: (Must equal 'candidate' (case insensitive) OR Must equal 'force' (case insensitive))


- **Remarks:** System Parameter for peers discovered out-of-band by the SDK user  Summary: Each time a new peer is discovered or an existing peer disappears, update this system parameter with all the currently discovered peers.  Use the `system:transports_info` virtual collection to get the discovery hint for a peer. Example: `SELECT * from system:transports_info where _id = 'discovery_hint'`  Schema:  ```text [{ address: String, (protocol + ip/hostname + port) type: enum(candidate|force), (optional, default: candidate) discovery_hint: String, (optional) }] ```  Example addresses: - "tcp://10.0.0.1:1234" - "tcp://[::1]:1234" - "tcp://my-device.local:1234" - "ws://192.168.1.100:8080" - "wss://my-ditto-server.example.com:443"  Usage:  ```text ALTER SYSTEM SET discovered_peers = [ { 'address': 'tcp://10.0.0.1:12345'}, { 'address': 'tcp://10.0.0.2:12345', 'type': 'force'}, { 'address': 'tcp://10.0.0.3:12346', 'type': 'candidate', 'discovery_hint': 'XXXXXXXXXX'}, { 'address': 'ws://192.168.1.100:8080', 'type': 'candidate'}, { 'address': 'wss://my-server.example.com:443', 'type': 'force'} ] ```

### transports_dns_sd_browse_enabled
- **Type:** `bool`
- **Default value:** true
- **Bounds:** No additional restrictions on the value
- **Remarks:** When LAN is enabled, controls whether Ditto will search for other peers using DNS-SD.

### transports_dns_sd_registration_enabled
- **Type:** `bool`
- **Default value:** true
- **Bounds:** No additional restrictions on the value
- **Remarks:** When LAN is enabled, controls whether Ditto will register (advertise) this peer using DNS-SD.

### transports_tcp_so_nodelay
- **Type:** `bool`
- **Default value:** true
- **Bounds:** No additional restrictions on the value
- **Remarks:** TCP Socket Option: TCP_NODELAY  If set, disable the Nagle algorithm: <https://en.wikipedia.org/wiki/Nagle's_algorithm>. This means that segments are always sent as soon as possible, reducing data delivery latency at the expense of an increased network utilization in case of small data. When not set, data is buffered until there is a sufficient amount to send out, thereby avoiding the frequent sending of small packets at the expense of increased latency.

### transports_udp_port
- **Type:** `u16`
- **Default value:** 4040
- **Bounds:** At least 0 (inclusive) and at most 65535 (inclusive)
- **Remarks:** UDP Server Port  The port to use for the UDP server. When set to 0, the OS will automatically assign an available port.

### transports_websocket_include_jwt_in_header
- **Type:** `bool`
- **Default value:** false
- **Bounds:** No additional restrictions on the value
- **Remarks:** Forces the websocket client to include the JWT in the `X-DITTO-AUTH` HTTP header (it will still be _also_ included in the body) in all future requests (does not require a restart).  This option is only available on non-WASM targets, as browsers do not allow setting headers.  This option comes with major downsides. If you weren't informed about this setting directly by Ditto support, you almost certainly do _not_ want to use it.

### transports_wifi_aware_background_mode
- **Type:** `ditto_configuration::types::TransportWifiAwareBackgroundMode`
- **Default value:** Off
- **Bounds:** No additional restrictions on the value
- **Remarks:** State Of WiFi Aware When The Application Goes To Background  When an application goes into background mode, Android may terminate WiFi Aware after some time to conserve battery. If this happens, WiFi Aware might not restart the next time the application returns to foreground mode. To prevent this issue, Ditto actively turns off WiFi Aware by default when it detects the application has entered background mode.  This system parameter controls the state of WiFi Aware when the application goes into background.  The default setting is to always turn off WiFi Aware when the application goes to background, which is the most conservative choice. However, this results in the loss of WiFi Aware connections immediately after the app transitions to background.  If you don't experience the loss of WiFi Aware after application background/foreground cycles, setting this parameter to `BestEffortOn` or `OffOnBatteryOnly` can help maintain WiFi Aware connections longer.

### transports_wifi_aware_client_is_enabled
- **Type:** `bool`
- **Default value:** true
- **Bounds:** No additional restrictions on the value
- **Remarks:** Enables or disables WiFi Aware client functionality.  Defaults to `true`. When set to `false`, this parameter disables the WiFi Aware transport's ability to scan and join WiFi Aware networks.  This value must be set before calling `startSync()`. Unlike other system parameters, this system parameter does not live-update if changed after sync is already started.  This should only be set to `false` if the device's hardware is experiencing issues with joining WiFi Aware networks. In most cases, this is unnecessary. On a mesh network, it is recommended to disable this setting on select nodes only, allowing other nodes to continue scanning and joining WiFi Aware networks.

### transports_wifi_aware_max_data_paths_older_devices
- **Type:** `u64`
- **Default value:** 4
- **Bounds:** At least 1 (inclusive) and at most 4294967295 (inclusive)
- **Remarks:** Maximum number of concurrent data paths allowed for Wi-Fi Aware on older Android devices (API < 31).  This parameter controls the limit for simultaneous Wi-Fi Aware connections on devices running Android versions older than API 31 (Android 12). On API 31+, the system API provides real-time availability information, so this parameter is not used.  The default value is 4, which is a conservative limit to prevent resource exhaustion. Users can adjust this based on their device capabilities and requirements.  Note: This only affects Android devices with API level < 31. Modern devices use the system's `availableDataPathsCount` API for accurate real-time tracking.

### transports_wifi_aware_max_error_count
- **Type:** `u64`
- **Default value:** 5
- **Bounds:** At least 1 (inclusive) and at most 4294967295 (inclusive)
- **Remarks:** The number of error events we observe before determining that WiFi Aware has become stale  This parameter, along with `WIFI_AWARE_RECENT_ERROR_DURATION_MS`, determines how WiFi Aware detects staleness.  During operation, the WiFi Aware subsystem can become stale, which means it may stop responding to network changes and requests. This ultimately leads to a complete loss of WiFi Aware connections. We detect this condition by counting the number of error events within the specified time period. When staleness is detected, WiFi Aware will restart itself to resolve the issue.  Decreasing this `WIFI_AWARE_MAX_ERROR_COUNT` and/or `WIFI_AWARE_RECENT_ERROR_DURATION_MS` parameters makes WiFi Aware more sensitive to errors, causing it to restart quicker and more frequently. Setting them too low, however, would risk having more false positives (WiFi Aware thinks it's stale while it's actually not).  Increasing them, on the other hand, would reduce false positive at the cost of taking longer to react to staleness.

### transports_wifi_aware_max_recent_error_duration_ms
- **Type:** `ditto_configuration::types::Milliseconds`
- **Default value:** Milliseconds(300000)
- **Bounds:** No additional restrictions on the value
- **Remarks:** The time period in milliseconds during which we consider WiFi Aware error events to be recent.  See `WIFI_AWARE_MAX_ERROR_COUNT` for detailed comments.

### transports_wifi_aware_server_is_enabled
- **Type:** `bool`
- **Default value:** true
- **Bounds:** No additional restrictions on the value
- **Remarks:** Enables or disables WiFi Aware server functionality.  Defaults to `true`. When set to `false`, this parameter disables the WiFi Aware transport's ability to advertise and create WiFi Aware networks.  This value must be set before calling `startSync()`. Unlike other system parameters, this system parameter does not live-update if changed after sync is already started.  This should only be set to `false` if the device's hardware is experiencing issues with creating WiFi Aware networks. In most cases, this is unnecessary. On a mesh network, it is recommended to disable this setting on select nodes only, allowing other nodes to continue advertising and creating WiFi Aware networks.

### udp_server_enabled
- **Type:** `bool`
- **Default value:** true
- **Bounds:** No additional restrictions on the value
- **Remarks:** UDP Server Enabled  Controls whether the UDP server should be enabled. When set to true (default), the UDP server will be automatically started alongside the TCP server.

### user_collection_sync_scopes
- **Type:** `alloc::collections::btree::map::BTreeMap<alloc::string::String, ditto_types::sync_scope::SyncScope>`
- **Default value:** {}
- **Bounds:** MapValidator with the following:
Keys must satisfy:
	NOT (Must start with '__')
Values must satisfy:
	(((Must equal 'AllPeers' (case insensitive) OR Must equal 'LocalPeerOnly' (case insensitive)) OR Must equal 'SmallPeersOnly' (case insensitive)) OR Must equal 'BigPeerOnly' (case insensitive))

- **Remarks:** The user defined mapping of collection names to SyncScopes which may be set using `ALTER SYSTEM`

### virt_conn_check_interval_ms
- **Type:** `ditto_configuration::types::Milliseconds`
- **Default value:** Milliseconds(15000)
- **Bounds:** No additional restrictions on the value
- **Remarks:** The interval between checks for idle VirtualConns. This value must be set at startup, runtime changes will be ignored.

### virt_conn_idle_timeout_ms
- **Type:** `ditto_configuration::types::Milliseconds`
- **Default value:** Milliseconds(60000)
- **Bounds:** No additional restrictions on the value
- **Remarks:** The time to keep a VirtualConn alive after the last PhysicalConn has gone.

### write_txn_trace_interval_ms
- **Type:** `u64`
- **Default value:** 10000
- **Bounds:** At least 1 (inclusive)
- **Remarks:** The interval of time between progressive trace statements if a write transaction cannot be acquired.  Specified as milliseconds instead of seconds in case we want to log more frequently than once a second. The default is 10 seconds.  Attempting to set the interval to 0 would trigger a panic, so this has a lower bound of 1.
