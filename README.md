# Breaknet

Mapping intranet address to public network

```text
+--------------+           |                 +--------+
|Public network|<----------X---------------->|Intranet|
+--------------+           |                 +--------+
      A                    |                    A
      |                   NAT                   |
      V                    |                    V
    +--------+             |                +--------+
    |bnserver|<---------------------------->|bnclient|
    +--------+                              +--------+
```

## build

`cargo build --release`

## Server

`./target/release/bnserver ./bnserver/config.json`

## Client

`./target/release/bnclient ./bnclient/config.json`

## Server config

```json
{
    "server": {
        "key": "helloworld",
        "port": 8808,
        "-limit-port": [
            9100,
            9110
        ]
    }
}
```

## Client config

```json
{
    "client": {
        "key": "helloworld",
        "server": "127.0.0.1:8808",
        "map": [
            {
                "inner": "127.0.0.1:6379",
                "outer": 9100
            },
            {
                "inner": "127.0.0.1:80",
                "outer": 9101
            }
        ]
    }
}
```

**meaning**

```text
127.0.0.1:9100 = 127.0.0.1:6379
127.0.0.1:9101 = 127.0.0.1:80
```

