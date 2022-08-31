#[allow(dead_code)]
mod ccd_codec;

use clap::{Args, Parser, Subcommand};
use color_eyre::{
    eyre::eyre,
    Result,
};
use colored::*;
use futures::{sink::SinkExt, stream::StreamExt};
use std::{path::Path, time::Duration};
use tokio::time::sleep;
use tokio_serial::{available_ports, SerialPortBuilderExt, SerialPortInfo, SerialStream};
use tokio_util::codec::{Decoder, Framed};

use ccd_codec::{handle_ccd_response, CCDCodec, Command as CCDCommand, Response as CCDResponse};

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Lists connected serial devices
    List,
    /// Get version info from CCD
    CCDVersion(SerialConf),
    /// Gets readings from spectrometer
    Read(ReadingConf),
}

#[derive(Args)]
struct SerialConf {
    /// Name of serial port that should be used
    #[clap(short, long, value_parser)]
    serial: String,
}

#[derive(Args)]
struct ReadingConf {
    /// Duration in seconds for which frames are continiously captured
    #[clap(short, long, value_parser, default_value = "3")]
    duration: u8,
    /// Path to a file where readings should be stored
    #[clap(short, long, value_parser, value_hint = clap::ValueHint::FilePath)]
    output: String,

    #[clap(flatten)]
    serial: SerialConf,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::List => list_serial(),
        Commands::CCDVersion(conf) => get_version(conf).await,
        Commands::Read(conf) => get_readings(conf).await,
    }
}

#[cfg(target_os = "linux")]
fn port_to_path(port: &SerialPortInfo) -> Result<String> {
    let dev_path = port
        .port_name
        .split('/')
        .last()
        .map(|d| format!("/dev/{}", d))
        .ok_or(eyre!("Could not map /sys/class/tty to /dev"))?;
    if Path::new(dev_path.as_str()).exists() {
        Ok(dev_path)
    } else {
        // It's quite possibe that udev can rename tty devices while mapping from /sys to /dev, but
        // I just don't want to link against libudev, this is supposed to be a small static project
        Err(eyre!(
            "Could not find port {}, even though {} exists",
            dev_path,
            port.port_name
        ))
    }
}

#[cfg(not(target_os = "linux"))]
fn port_to_path(port: &SerialPortInfo) -> Result<String> {
    Ok(port.port_name.clone())
}

fn get_port_paths() -> Result<Vec<String>> {
    let ports = available_ports()?;
    ports
        .iter()
        .map(port_to_path)
        .filter(|path_res| match path_res {
            Ok(path) => !path.is_empty(),
            Err(_) => false,
        })
        .collect()
}

fn list_serial() -> Result<()> {
    let paths = get_port_paths()?;
    if paths.is_empty() {
        println!("{}", "No connected serial ports found.".red())
    } else {
        println!("{}", "Connected serial ports:".green());
        paths.iter().for_each(|p| println!("{}", p));
    }

    Ok(())
}

fn open_ccd_connection(conf: &SerialConf) -> Result<Framed<SerialStream, CCDCodec>> {
    // TODO: Dynamically change baudrate
    let mut port = tokio_serial::new(conf.serial.clone(), 115200).open_native_async()?;

    #[cfg(unix)]
    port.set_exclusive(false)?;

    Ok(ccd_codec::CCDCodec.framed(port))
}

async fn get_readings(conf: &ReadingConf) -> Result<()> {
    let mut frames: Vec<_> = Vec::new();
    let timeout = sleep(Duration::from_secs(conf.duration.into()));
    tokio::pin!(timeout);

    let mut ccd = open_ccd_connection(&conf.serial)?;

    ccd.send(CCDCommand::ContinuousRead).await?;
    loop {
        tokio::select! {
            resp = ccd.next() => {
                handle_ccd_response!(
                    resp, CCDResponse::SingleReading,
                    |frame: ccd_codec::Frame| {frames.push(frame)}
                )?;
            },
            _ = &mut timeout => {
                break;
            }
        }
    }
    ccd.send(CCDCommand::PauseRead).await?;

    Ok(())
}

async fn get_version(conf: &SerialConf) -> Result<()> {
    let mut ccd = open_ccd_connection(&conf)?;

    ccd.send(CCDCommand::GetVersion).await?;
    handle_ccd_response!(
        ccd.next().await,
        CCDResponse::VersionInfo,
        |info: ccd_codec::VersionDetails| {
            println!("Hardware version: {}", info.hardware_version);
            println!("Firmware version: {}", info.firmware_version);
            println!("Sensor type: {}", info.sensor_type);
            println!("Serial number: {}", info.serial_number);
        }
    )?;

    Ok(())
}
