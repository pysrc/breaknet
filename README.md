# Breaknet

向日葵、Frp之类的实现

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

运行前需要生成ssl认证文件，运行程序：gencertificate

bnserver是服务端程序，bnclient是客户端程序

产物结构如下

```
bnserver.exe
bnserver-config.yml
cert.pem
key.pem
```

```
bnclient.exe
bnclient-config.yml
cert.pem
```


# 配置样式

## bnserver-config.yml

```yml
bind: 127.0.0.1:8808
ssl-cert: ./cert.pem
ssl-key: ./key.pem
```

## bnclient-config.yml

```yml
# 服务端地址
server: 127.0.0.1:8808
# ssl公钥
ssl-cert: ./cert.pem
# 内外部地址映射
# inner代表内网地址
# outer代表服务端绑定的地址
map:
  - inner: 127.0.0.1:8000
    outer: 0.0.0.0:9000
  - inner: 127.0.0.1:8000
    outer: 0.0.0.0:9100
```
