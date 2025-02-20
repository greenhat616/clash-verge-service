use axum::{Router, http::Request, routing::get};
use hyper::body::Incoming;
use hyper_util::{
    rt::{TokioExecutor, TokioIo},
    server,
};
use interprocess::local_socket::{ListenerNonblockingMode, ListenerOptions, tokio::prelude::*};
#[cfg(unix)]
use interprocess::os::unix::local_socket::ListenerOptionsExt;
#[cfg(windows)]
use interprocess::os::windows::{
    local_socket::ListenerOptionsExt, security_descriptor::SecurityDescriptor,
};
use nyanpasu_utils::io::unwrap_infallible;
use std::result::Result as StdResult;
use thiserror::Error;
use tower::Service;

mod ws;

type Result<T> = StdResult<T, ServerError>;

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Other error: {0}")]
    Other(#[from] anyhow::Error),
}
use tracing_attributes::instrument;

#[instrument]
pub async fn create_server(placeholder: &str, app: Router) -> Result<()> {
    let name = crate::utils::get_name(placeholder)?;
    #[cfg(unix)]
    {
        crate::utils::remove_socket_if_exists(placeholder).await?;
    }
    tracing::debug!("socket name: {:?}", name);
    let options = ListenerOptions::new()
        .name(name)
        .nonblocking(ListenerNonblockingMode::Both);
    #[cfg(windows)]
    let options = {
        use widestring::u16cstr;
        let sdsf = u16cstr!("D:(A;;GA;;;WD)"); // TODO: allow only the permitted users to connect
        let sw = SecurityDescriptor::deserialize(sdsf)?;
        options.security_descriptor(sw)
    };
    // allow owner and group to read and write
    #[cfg(unix)]
    let options = options.mode({
        #[cfg(target_os = "linux")]
        {
            0o664 as u32
        }
        #[cfg(not(target_os = "linux"))]
        {
            0o664 as u16
        }
    });

    let listener = options.create_tokio()?;
    // change the socket group
    tracing::debug!("changing socket group and permissions...");
    crate::utils::os::change_socket_group(placeholder)?;
    crate::utils::os::change_socket_mode(placeholder)?;

    tracing::debug!("mounting service...");
    let mut make_service = app.route("/ws", get(ws::ws_handler)).into_make_service();
    // See https://github.com/tokio-rs/axum/blob/main/examples/serve-with-hyper/src/main.rs for
    // more details about this setup
    loop {
        let socket = listener.accept().await?;
        let tower_service = unwrap_infallible(make_service.call(&socket).await);

        tokio::spawn(async move {
            let socket = TokioIo::new(socket);

            let hyper_service = hyper::service::service_fn(move |request: Request<Incoming>| {
                tower_service.clone().call(request)
            });

            if let Err(err) = server::conn::auto::Builder::new(TokioExecutor::new())
                .serve_connection_with_upgrades(socket, hyper_service)
                .await
            {
                tracing::error!("failed to serve connection: {err:#}");
            }
        });
    }
}
