use tokio::sync::mpsc::Sender;

pub trait Input {
    fn start(&mut self, sender: &Sender<Vec<u8>>) -> Result<(), Box<dyn std::error::Error>>;
    fn stop(&self) -> Result<(), Box<dyn std::error::Error>>;
}
