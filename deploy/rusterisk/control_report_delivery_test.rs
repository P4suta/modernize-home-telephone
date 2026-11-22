#[tokio::test]
async fn failed_control_delivery_preserves_rtp() -> Result<(), MediaError> {
    let peer = SocketPair::bind(Ipv4Addr::LOCALHOST.into(), 0, 0).map_err(failure)?;
    let mut phone = fixture(&peer)?;
    let control = tokio::net::UdpSocket::bind((std::net::Ipv6Addr::LOCALHOST, 0))
        .await
        .map_err(failure)?;
    assert!(control
        .send_to(&[0], peer.control_addr().map_err(failure)?)
        .await
        .is_err());
    phone.link.sockets.control = control;
    let task = tokio::spawn(phone.link.run());
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
    assert!(RtpPacket::parse(&packet[..size]).is_ok());
    phone.stop.send_replace(true);
    assert_eq!(finish(task).await?.packets_sent, 1);
    Ok(())
}
