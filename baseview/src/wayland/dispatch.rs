//! Bounded socket waits for application-owned Wayland event queues.
use std::{io::ErrorKind, os::fd::AsRawFd, time::Duration};

use nix::poll::{poll, PollFd, PollFlags};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Instant;
use wayland_client::{backend::WaylandError, EventQueue};
use wayland_client::{protocol::wl_callback, Connection, Dispatch, QueueHandle};

pub(crate) fn roundtrip<S>(
    connection: &Connection,
    queue: &mut EventQueue<S>,
    state: &mut S,
    timeout: Duration,
) -> Result<(), String>
where
    S: Dispatch<wl_callback::WlCallback, Arc<AtomicBool>> + 'static,
{
    let done = Arc::new(AtomicBool::new(false));
    connection.display().sync(&queue.handle(), done.clone());
    let deadline = Instant::now() + timeout;
    while !done.load(Ordering::Acquire) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("Wayland handshake timed out".into());
        }
        dispatch_for(queue, state, remaining.min(Duration::from_millis(20)))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct State;
    impl Dispatch<wl_callback::WlCallback, Arc<AtomicBool>> for State {
        fn event(
            _: &mut Self,
            _: &wl_callback::WlCallback,
            _: wl_callback::Event,
            done: &Arc<AtomicBool>,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            done.store(true, Ordering::Release);
        }
    }
    #[test]
    fn an_unresponsive_peer_times_out() {
        let (client, _peer) = std::os::unix::net::UnixStream::pair().unwrap();
        let connection = Connection::from_socket(client).unwrap();
        let mut queue = connection.new_event_queue();
        let error = roundtrip(
            &connection,
            &mut queue,
            &mut State,
            Duration::from_millis(30),
        )
        .unwrap_err();
        assert_eq!(error, "Wayland handshake timed out");
    }
}

pub(crate) fn dispatch_for<S>(
    queue: &mut EventQueue<S>,
    state: &mut S,
    timeout: Duration,
) -> Result<(), String> {
    if queue.dispatch_pending(state).map_err(|e| e.to_string())? > 0 {
        return Ok(());
    }
    let writable = match queue.flush() {
        Ok(()) => false,
        Err(WaylandError::Io(e)) if e.kind() == ErrorKind::WouldBlock => true,
        Err(e) => return Err(e.to_string()),
    };
    // Prepare before polling, as required by libwayland's read protocol.
    let Some(guard) = queue.prepare_read() else {
        queue.dispatch_pending(state).map_err(|e| e.to_string())?;
        return Ok(());
    };
    let mut interest = PollFlags::POLLIN;
    if writable {
        interest |= PollFlags::POLLOUT;
    }
    let mut fds = [PollFd::new(guard.connection_fd().as_raw_fd(), interest)];
    let milliseconds = timeout.as_millis().min(i32::MAX as u128) as i32;
    match poll(&mut fds, milliseconds) {
        Ok(_) => {}
        Err(nix::errno::Errno::EINTR) => return Ok(()),
        Err(e) => return Err(e.to_string()),
    }
    let ready = fds[0].revents().unwrap_or_else(PollFlags::empty);
    if ready.intersects(PollFlags::POLLERR | PollFlags::POLLHUP | PollFlags::POLLNVAL) {
        return Err(format!("Wayland socket disconnected: {ready:?}"));
    }
    if ready.contains(PollFlags::POLLIN) {
        match guard.read() {
            Ok(_) => {}
            Err(WaylandError::Io(e)) if e.kind() == ErrorKind::WouldBlock => {}
            Err(e) => return Err(e.to_string()),
        }
    } else {
        drop(guard);
    }
    if ready.contains(PollFlags::POLLOUT) {
        match queue.flush() {
            Ok(()) => {}
            Err(WaylandError::Io(e)) if e.kind() == ErrorKind::WouldBlock => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    queue.dispatch_pending(state).map_err(|e| e.to_string())?;
    Ok(())
}
