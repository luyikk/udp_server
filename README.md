# udp_server
fast rust udp server


''' Examples

  let mut a = UdpServer::new("0.0.0.0:5555").await.unwrap();
  a.set_input(async move |peer,data|{
        let mut un_peer = peer.lock().await;
        match &un_peer.token {
            Some(x)=>{
                *x.borrow_mut()+=1;
                },
            None=>{
                un_peer.token=Some(RefCell::new(1));
            }
        }
        un_peer.send(&data).await?;
        OK(())
    });    
  a.start().await.unwrap();
  
'''
