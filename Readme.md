## A camera streaming application

## Install
```shell
# Centos
yum install opencv
```

## Server
```shell
cargo run --bin server
```

## Client(local)
```shell
cargo run --bin client --  -t true -u xx -p xx -f 40 -q 60 -l label1
```


