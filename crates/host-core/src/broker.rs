use crate::task::{HostEvent,TaskSnapshot};
use tokio::sync::broadcast;

/// In-process bounded event broker. Writers publish once; readers recover from a snapshot.
#[derive(Clone)] pub struct EventBroker { tx:broadcast::Sender<HostEvent> }
impl EventBroker {
    pub fn new(capacity:usize)->Self { let (tx,_)=broadcast::channel(capacity.max(1)); Self{tx} }
    pub fn subscribe(&self)->broadcast::Receiver<HostEvent>{self.tx.subscribe()}
    pub fn publish(&self,event:HostEvent){let _=self.tx.send(event);}
    pub fn recover(snapshot:&TaskSnapshot,after_seq:u64)->Vec<HostEvent>{snapshot.events.iter().filter(|e|e.seq>after_seq).cloned().collect()}
}
