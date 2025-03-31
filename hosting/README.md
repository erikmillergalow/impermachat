```
cargo build --release
sudo cp target/release/impermachat /opt/impermachat/
sudo cp -r assets/ /opt/impermachat/
sudo chown -R impermachat:impermachat /opt/impermachat/
sudo chmod -R 750 /opt/impermachat/
sudo systemctl start impermachat.service 
```
