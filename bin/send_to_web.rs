use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task;

async fn handle_client(mut socket: TcpStream) -> tokio::io::Result<()> {
    let mut buffer = vec![0; 1024];

    loop {
        let n = socket.read(&mut buffer).await?;
        if n == 0 {
            break; // 客户端关闭连接
        }

        // 回显收到的数据
        socket.write_all(&buffer[..n]).await?;
    }

    Ok(())
}

async fn start_server(addr: &str) -> tokio::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    println!("Listening on {}", addr);

    loop {
        let (socket, _) = listener.accept().await?;
        println!("New connection on {}", addr);

        // 启动一个任务处理连接
        task::spawn(async move {
            if let Err(e) = handle_client(socket).await {
                eprintln!("Failed to handle client: {}", e);
            }
        });
    }
}

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    let addr1 = "127.0.0.1:8080";
    let addr2 = "127.0.0.1:8081";

    // 启动两个异步 TCP 服务
    let server1 = start_server(addr1);
    let server2 = start_server(addr2);

    // 同时运行两个服务
    tokio::try_join!(server1, server2)?;

    Ok(())
}
