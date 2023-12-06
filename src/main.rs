mod subcommands;

use argh::FromArgs;
use quic_tunnel::log::configure_logging;
use subcommands::{
    QuickCertsSubCommand, ReverseProxyClientSubCommand, ReverseProxyServerSubCommand,
    UdpClientSubCommand, UdpServerSubCommand,
};

#[derive(FromArgs, PartialEq, Debug)]
/// Top-level command.
struct TopLevel {
    #[argh(subcommand)]
    nested: MySubCommandEnum,
}

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand)]
enum MySubCommandEnum {
    QuickCerts(QuickCertsSubCommand),
    ReverseProxyClient(ReverseProxyClientSubCommand),
    ReverseProxyServer(ReverseProxyServerSubCommand),
    UdpClient(UdpClientSubCommand),
    UdpServer(UdpServerSubCommand),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let command: TopLevel = argh::from_env();

    configure_logging();

    match command.nested {
        MySubCommandEnum::QuickCerts(subcommand) => subcommand.main()?,
        MySubCommandEnum::ReverseProxyClient(subcommand) => subcommand.main().await?,
        MySubCommandEnum::ReverseProxyServer(subcommand) => subcommand.main().await?,
        MySubCommandEnum::UdpClient(subcommand) => subcommand.main().await?,
        MySubCommandEnum::UdpServer(subcommand) => subcommand.main().await?,
    }

    Ok(())
}
