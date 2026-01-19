+++
title = "Kubernetes"
weight = 1
slug = "k8s"
insert_anchor_links = "heading"
+++

A Kubernetes Deployment in YAML vs Styx.

```compare
/// yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: web-app
  namespace: production
  labels:
    app: web
    tier: frontend
spec:
  replicas: 3
  selector:
    matchLabels:
      app: web
  template:
    metadata:
      labels:
        app: web
    spec:
      containers:
        - name: nginx
          image: nginx:1.25
          ports:
            - containerPort: 80
          env:
            - name: API_URL
              value: "https://api.example.com"
            - name: DB_PASSWORD
              valueFrom:
                secretKeyRef:
                  name: db-secrets
                  key: password
          resources:
            limits:
              cpu: "500m"
              memory: "256Mi"
            requests:
              cpu: "100m"
              memory: "128Mi"
          volumeMounts:
            - name: config
              mountPath: /etc/nginx/conf.d
              readOnly: true
      volumes:
        - name: config
          configMap:
            name: nginx-config
/// styx
apiVersion apps/v1
kind Deployment
metadata {
  name web-app
  namespace production
  labels app>web tier>frontend
}
spec {
  replicas 3
  selector.matchLabels app>web
  template {
    metadata.labels app>web
    spec {
      containers ({
        name nginx
        image nginx:1.25
        ports ({containerPort 80})
        env (
          {name API_URL, value https://api.example.com}
          {
            name DB_PASSWORD
            valueFrom.secretKeyRef name>db-secrets key>password
          }
        )
        resources {
          limits cpu>500m memory>256Mi
          requests cpu>100m memory>128Mi
        }
        volumeMounts ({
          name config
          mountPath /etc/nginx/conf.d
          readOnly true
        })
      })
      volumes ({
        name config
        configMap.name nginx-config
      })
    }
  }
}
```
