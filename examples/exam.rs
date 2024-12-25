#[tokio::main]
pub async fn main() {
    let (s, r) = async_channel::unbounded();
    assert_eq!(s.send("Hello").await, Ok(()));
    assert_eq!(r.recv().await, Ok("Hello"));
}
