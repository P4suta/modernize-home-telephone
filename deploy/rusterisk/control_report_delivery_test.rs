use tracing::instrument::WithSubscriber as _;
use tracing_subscriber::prelude::*;

struct ControlFailures(mpsc::UnboundedSender<()>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for ControlFailures {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        if event.metadata().name() == "rtcp_send_failed" {
            let _ = self.0.send(());
        }
    }
}

#[tokio::test]
async fn failed_control_delivery_preserves_rtp() -> Result<(), MediaError> {
    let peer = SocketPair::bind(Ipv4Addr::LOCALHOST.into(), 0, 0).map_err(failure)?;
    let mut phone = fixture(&peer)?;
    let control = tokio::net::UdpSocket::bind((std::net::Ipv6Addr::LOCALHOST, 0))
        .await
        .map_err(failure)?;
    assert!(
        control
            .send_to(&[0], peer.control_addr().map_err(failure)?)
            .await
            .is_err()
    );
    phone.link.sockets.control = control;
    let (observer, mut failures) = mpsc::unbounded_channel();
    let subscriber = tracing_subscriber::registry().with(ControlFailures(observer));
    let task = tokio::spawn(phone.link.run().with_subscriber(subscriber));
    phone
        .outgoing
        .send(Frame::new(&[750; 160], 8000)?)
        .await
        .map_err(failure)?;
    let mut packet = [0; 256];
    let (size, _) =
        tokio::time::timeout(Duration::from_secs(2), peer.data().recv_from(&mut packet))
            .await
            .map_err(failure)?
            .map_err(failure)?;
    let first = RtpPacket::parse(&packet[..size])?.sequence();
    tokio::time::timeout(Duration::from_secs(6), failures.recv())
        .await
        .map_err(failure)?
        .ok_or_else(|| failure("missing failed periodic RTCP report"))?;
    phone
        .outgoing
        .send(Frame::new(&[1500; 160], 8000)?)
        .await
        .map_err(failure)?;
    let (size, _) =
        tokio::time::timeout(Duration::from_secs(2), peer.data().recv_from(&mut packet))
            .await
            .map_err(failure)?
            .map_err(failure)?;
    assert_eq!(
        RtpPacket::parse(&packet[..size])?.sequence(),
        first.wrapping_add(1)
    );
    phone.stop.send_replace(true);
    assert_eq!(finish(task).await?.packets_sent, 2);
    failures.try_recv().map_err(failure)?;
    Ok(())
}
