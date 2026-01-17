+++
title = "nginx"
weight = 9
slug = "nginx"
insert_anchor_links = "heading"
+++

An nginx configuration vs Styx.

```compare
/// nginx
worker_processes auto;
error_log /var/log/nginx/error.log warn;
pid /run/nginx.pid;

events {
    worker_connections 1024;
    use epoll;
    multi_accept on;
}

http {
    include       /etc/nginx/mime.types;
    default_type  application/octet-stream;

    log_format main '$remote_addr - $remote_user [$time_local] "$request" '
                    '$status $body_bytes_sent "$http_referer" '
                    '"$http_user_agent"';

    access_log /var/log/nginx/access.log main;

    sendfile        on;
    tcp_nopush      on;
    tcp_nodelay     on;
    keepalive_timeout 65;
    gzip on;
    gzip_types text/plain text/css application/json application/javascript;

    upstream backend {
        least_conn;
        server 127.0.0.1:3001 weight=3;
        server 127.0.0.1:3002 weight=2;
        server 127.0.0.1:3003 backup;
    }

    server {
        listen 80;
        listen [::]:80;
        server_name example.com www.example.com;
        return 301 https://$server_name$request_uri;
    }

    server {
        listen 443 ssl http2;
        listen [::]:443 ssl http2;
        server_name example.com www.example.com;

        ssl_certificate     /etc/ssl/certs/example.com.crt;
        ssl_certificate_key /etc/ssl/private/example.com.key;
        ssl_protocols       TLSv1.2 TLSv1.3;
        ssl_ciphers         HIGH:!aNULL:!MD5;

        root /var/www/html;
        index index.html;

        location / {
            try_files $uri $uri/ /index.html;
        }

        location /api/ {
            proxy_pass http://backend;
            proxy_set_header Host $host;
            proxy_set_header X-Real-IP $remote_addr;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto $scheme;
        }

        location ~* \.(js|css|png|jpg|jpeg|gif|ico|svg|woff2)$ {
            expires 1y;
            add_header Cache-Control "public, immutable";
        }
    }
}
/// styx
worker_processes auto
error_log /var/log/nginx/error.log warn
pid /run/nginx.pid

events {
  worker_connections 1024
  use epoll
  multi_accept on
}

http {
  include /etc/nginx/mime.types
  default_type application/octet-stream

  log_format.main <<FMT
    $remote_addr - $remote_user [$time_local] "$request"
    $status $body_bytes_sent "$http_referer" "$http_user_agent"
    FMT

  access_log /var/log/nginx/access.log main

  sendfile on
  tcp_nopush on
  tcp_nodelay on
  keepalive_timeout 65
  gzip on
  gzip_types (text/plain text/css application/json application/javascript)

  upstream.backend {
    least_conn @
    server 127.0.0.1:3001 weight>3
    server 127.0.0.1:3002 weight>2
    server 127.0.0.1:3003 backup
  }

  // Redirect HTTP to HTTPS
  server {
    listen (80 [::]:80)
    server_name (example.com www.example.com)
    return "301 https://$server_name$request_uri"
  }

  // Main HTTPS server
  server {
    listen (443/ssl/http2 [::]:443/ssl/http2)
    server_name (example.com www.example.com)

    ssl_certificate /etc/ssl/certs/example.com.crt
    ssl_certificate_key /etc/ssl/private/example.com.key
    ssl_protocols (TLSv1.2 TLSv1.3)
    ssl_ciphers HIGH:!aNULL:!MD5

    root /var/www/html
    index index.html

    location./ try_files "$uri $uri/ /index.html"

    location./api/ {
      proxy_pass http://backend
      proxy_set_header (
        Host>$host
        X-Real-IP>$remote_addr
        X-Forwarded-For>$proxy_add_x_forwarded_for
        X-Forwarded-Proto>$scheme
      )
    }

    location.static r#"\.(js|css|png|jpg|jpeg|gif|ico|svg|woff2)$"# {
      expires 1y
      add_header Cache-Control>"public, immutable"
    }
  }
}
```
