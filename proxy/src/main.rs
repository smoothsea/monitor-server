mod server;

use server::Server;
use anyhow::Result;


#[tokio::main]
async fn main() -> Result<()> {
    let server = Server::new();
    server.listen().await?;
    
    Ok(())
}
