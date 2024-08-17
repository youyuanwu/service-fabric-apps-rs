use bytes::Buf;
use mssf_core::{
    runtime::{stateful_types::Epoch, store_types::ReplicatorSettings},
    sync::FabricReceiver,
};

use crate::types::OperationMetadata;

/// Defines the behavior that a service must implement to interact with the FabricReplicator.
#[trait_variant::make(StateProvider: Send)]
pub trait LocalStateProvider: Sync + 'static {
    /// Indicates to a replica that the configuration of a replica
    /// set has changed due to a change or attempted change to the
    /// primary replica. The change occurs due to failure or load balancing
    /// of the previous primary replica.
    /// Epoch changes act as a barrier by segmenting operations into the
    /// exact configuration periods in which they were sent by a specific primary replica.
    async fn update_epoch(
        &self,
        epoch: &Epoch,
        previousepochlastsequencenumber: i64,
    ) -> mssf_core::Result<()>;

    /// Obtains the last sequence number that the service has committed,
    /// also known as Logical Sequence Number (LSN).
    fn get_last_committed_sequence_number(&self) -> mssf_core::Result<i64>;

    /// Indicates that a write quorum of replicas in this replica set has been lost,
    /// and that therefore data loss might have occurred.
    /// The replica set consists of a majority of replicas, which includes the primary replica.
    async fn on_data_loss(&self) -> mssf_core::Result<bool>; // is state changed.

    /// Obtains context on a secondary replica after it is created
    /// and opened to send context to the primary replica.
    ///
    /// Remarks:The Primary replica analyzes the context and sends back state
    /// via getCopyState(SequenceNumber upToSequenceNumber, OperationDataStream copyContext).
    /// getCopyContext() is called on newly created, idle Secondary replicas and provides a
    /// mechanism to asynchronously establish a bidirectional conversation with the Primary replica.
    /// The Secondary replica sends OperationData objects with which the Primary replica can
    /// determine the progress of collecting context on the Secondary replica.
    /// The Primary replica responds by sending the required state back.
    /// See getCopyState(SequenceNumber upToSequenceNumber, OperationDataStream copyContext)
    /// at the Primary replica for the other half of the exchange.
    /// For in-memory services, the getCopyContext() method is not called, as the state of the
    /// Secondary replicas is known (they are empty and will require all of the state).
    fn get_copy_context(&self) -> mssf_core::Result<impl OperationDataStream>;

    /// Obtains state on a primary replica that is required to build a secondary replica.
    /// Just as getCopyContext() enables the Secondary replica to send context to the
    /// Primary replica via an OperationDataStream,
    /// getCopyState(SequenceNumber upToSequenceNumber, OperationDataStream copyContext)
    /// enables the Primary replica to respond with an OperationDataStream.
    /// The stream contains objects that are delivered to the Secondary replica via the
    /// getCopyStream() method of the FabricReplicator class. The objects implement Operation and contain the specified data.
    /// When the Primary replica receives this call, it should create and return another
    /// OperationDataStream that contains OperationData.
    /// OperationData represents the data/state that the Secondary replica
    ///
    /// Parameters:
    /// upToSequenceNumber - The maximum last sequence number (LSN) that should be
    /// placed in the copy stream via the getCopyStream() method.
    /// LSNs greater than this number are delivered to the Secondary replica as a
    /// part of the replication stream via the getReplicationStream() method.
    /// copyContext - An OperationDataStream that contains the OperationData
    /// objects that are created by the Secondary replica. requires to catch up to the provided
    fn get_copy_state(
        &self,
        upto_sequence_number: i64,
        copy_context_stream: impl OperationDataStream,
    ) -> mssf_core::Result<impl OperationDataStream>;
}

/// Exposes replication related functions of the FabricReplicator class that are
/// used by StateProvider to replicate state to ensure high availability.
pub trait StateReplicator {
    /// Replicates state changes from Primary replica to the Secondary replicas
    /// and receives a quorum acknowledgement that those state changes have been applied.
    ///
    /// TODO: figure out cancellation.
    /// Parameters:
    /// operationData - Represents the state change that the Primary replica wants to replicate.
    /// cancellationToken - A write quorum of replicas that have been lost.
    ///   It can be used to send a notification that the operation should be canceled.
    ///   Note that cancellation is advisory and that the operation might still be completed even if it is canceled.
    /// Returns:
    /// sequenceNumber - Long, the LSN of the operation. Note that this is the same value which is returned by the task.
    ///   Providing it as an out parameter is useful for services which want to prepare the local write to commit when the task finishes.
    /// Returns completable future of type long, the LSN of the operation.
    /// Remarks:
    /// This is a special fn signature because the sn needs to be returned immediately. The future is returned as a receiver.
    fn replicate(
        &self,
        operation_data: impl OperationData,
    ) -> (i64, FabricReceiver<mssf_core::Result<i64>>);

    /// Gets replication stream.
    /// Returns:
    /// The ReplicationStream implements OperationStream.
    /// The ReplicationStream contains OperationData objects that implement Operation.
    /// The objects are provided by the Primary replica via
    /// replicateAsync(OperationData operationData, SequenceNumber sequenceNumber, CancellationToken cancellationToken).
    /// Generally a Secondary replica should send getOperationAsync(CancellationToken cancellationToken).
    /// Although Service Fabric does not require services to do so, generally services should transfer all
    /// OperationData objects out of the copy stream first, and then transfer operations out of the replication stream.
    /// The transfer from both copies in parallel is supported but increases the complexity of applying state updates
    /// correctly and is recommended only for advanced services. The stream is empty when the returned Operation method is null.
    fn get_replication_stream(&self) -> mssf_core::Result<impl OperationStream>;

    /// Gets copy stream
    /// Returns:
    /// The returned CopyStream contains OperationData objects that implement Operation.
    /// The OperationData objects are obtained from the CopyState OperationDataStream that the
    /// Primary replica returns from getCopyState(SequenceNumber upToSequenceNumber, OperationDataStream copyContext).
    /// When a replica is created and has to catch up, it should obtain the CopyStream and begin to send, apply,
    /// and acknowledge the Copy objects that implement Operation. In parallel, the replica responds
    /// to the corresponding getCopyContext() and getNextAsync(CancellationToken cancellationToken)calls.
    /// The stream is empty when the returned Operation method is null.
    fn get_copy_stream(&self) -> mssf_core::Result<impl OperationStream>;

    /// Enables modification of replicator settings during runtime. The only setting which can be modified is the security credentials.
    /// Parameters:
    /// settings - The new ReplicatorSettings with updated credentials.
    fn update_replicator_settings(&self, settings: &ReplicatorSettings) -> mssf_core::Result<()>;

    /// Retrieves the replicator settings during runtime.
    /// Returns:
    /// The current ReplicatorSettings from the Service Fabric runtime.
    fn get_replicator_settings(&self) -> mssf_core::Result<ReplicatorSettings>;
}

// Operation data itself is Buf as well.
pub trait OperationData: Buf + Sync + Send + 'static {
    fn get_data(&self) -> mssf_core::Result<&impl Buf>;
}

#[trait_variant::make(OperationDataStream: Send)]
pub trait LocalOperationDataStream: Sync + 'static {
    // Returning null indicates to the system that the transfer is complete.
    async fn get_next(&self) -> mssf_core::Result<Option<impl OperationData>>;
}

// IFabricOperation
pub trait Operation: Send {
    /// Gets the type of this operation.
    /// Remarks:The OperationType indicates the type of operation.
    /// "Normal" operations are those operations that are sent by non-service
    /// grouped services as part of either the copy or replication streams.
    /// Other types of operations represent control operations that are specific to service groups.
    fn get_metadate(&self) -> OperationMetadata;

    /// Gets the OperationData that are provided by the Primary replica.
    fn get_data(&self) -> mssf_core::Result<impl Buf + Send>;

    /// Acknowledges that this operation has been successfully applied at the Secondary replica.
    /// Remarks:Services should call this method when they have obtained
    /// an system.fabric.Operation from the replicator and successfully applied
    /// it to their local store. For persisted services, calling this method is mandatory
    ///  because the FabricReplicator does not release additional objects that
    /// implement system.fabric.Operation. For volatile services, the replicator
    /// implicitly acknowledges operations when they are received unless they are
    /// configured otherwise by setting the value isRequireServiceAck() to true.
    /// An operation must be acknowledged by a quorum of replicas before the Primary
    /// replica receives the
    /// replicateAsync(OperationData operationData, SequenceNumber sequenceNumber, CancellationToken cancellationToken)
    /// operation complete responses.
    fn acknowledge(&self) -> mssf_core::Result<()>;
}

// Represents a stream of replication or copy operations that are sent
// from the Primary to the Secondary replica.
#[trait_variant::make(OperationStream: Send)]
pub trait LocalOperationStream {
    // returns null if end of stream.
    async fn get_operation(&self) -> mssf_core::Result<Option<impl Operation>>;
    fn report_fault(&self) -> mssf_core::Result<()>; // TODO:
}
