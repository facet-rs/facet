+++
title = "Schema Examples"
weight = 1
slug = "schema-examples"
insert_anchor_links = "heading"
+++

This document shows real-world schema examples with side-by-side YAML and STYX representations.

## Kubernetes Deployment

### Schema

```styx
meta {
  id https://styx-lang.org/schemas/k8s/deployment
  version 2026-01-12
}

schema {
  @ {
    apiVersion @string
    kind @string
    metadata @Metadata
    spec @DeploymentSpec
  }

  Metadata {
    name @string
    namespace @optional(@string)
    labels @optional(@map(@string))
    annotations @optional(@map(@string))
  }

  DeploymentSpec {
    replicas @optional(@u32)
    selector @Selector
    template @PodTemplate
  }

  Selector {
    matchLabels @map(@string)
  }

  PodTemplate {
    metadata @Metadata
    spec @PodSpec
  }

  PodSpec {
    containers (@Container)
    volumes @optional((@Volume))
    serviceAccountName @optional(@string)
    nodeSelector @optional(@map(@string))
  }

  Container {
    name @string
    image @string
    ports @optional((@ContainerPort))
    env @optional((@EnvVar))
    resources @optional(@Resources)
    volumeMounts @optional((@VolumeMount))
    command @optional((@string))
    args @optional((@string))
  }

  ContainerPort {
    name @optional(@string)
    containerPort @u16
    protocol @optional(@string)
  }

  EnvVar {
    name @string
    value @optional(@string)
    valueFrom @optional(@EnvVarSource)
  }

  EnvVarSource {
    secretKeyRef @optional(@SecretKeyRef)
    configMapKeyRef @optional(@ConfigMapKeyRef)
    fieldRef @optional(@FieldRef)
  }

  SecretKeyRef {
    name @string
    key @string
  }

  ConfigMapKeyRef {
    name @string
    key @string
  }

  FieldRef {
    fieldPath @string
  }

  Resources {
    limits @optional(@ResourceList)
    requests @optional(@ResourceList)
  }

  ResourceList {
    cpu @optional(@string)
    memory @optional(@string)
  }

  Volume {
    name @string
    configMap @optional(@ConfigMapVolume)
    secret @optional(@SecretVolume)
    emptyDir @optional(@EmptyDirVolume)
    persistentVolumeClaim @optional(@PvcVolume)
  }

  ConfigMapVolume {
    name @string
  }

  SecretVolume {
    secretName @string
  }

  EmptyDirVolume {
    @ @any
  }

  PvcVolume {
    claimName @string
  }

  VolumeMount {
    name @string
    mountPath @string
    readOnly @optional(@boolean)
  }
}
```

### YAML

```yaml
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
```

### STYX

```styx
apiVersion apps/v1
kind Deployment
metadata {
  name web-app
  namespace production
  labels { app web, tier frontend }
}
spec {
  replicas 3
  selector {
    matchLabels { app web }
  }
  template {
    metadata {
      labels { app web }
    }
    spec {
      containers (
        {
          name nginx
          image nginx:1.25
          ports ({ containerPort 80 })
          env (
            { name API_URL, value https://api.example.com }
            {
              name DB_PASSWORD
              valueFrom {
                secretKeyRef { name db-secrets, key password }
              }
            }
          )
          resources {
            limits { cpu 500m, memory 256Mi }
            requests { cpu 100m, memory 128Mi }
          }
          volumeMounts (
            { name config, mountPath /etc/nginx/conf.d, readOnly true }
          )
        }
      )
      volumes (
        { name config, configMap { name nginx-config } }
      )
    }
  }
}
```

### STYX (minified)

```styx
{apiVersion apps/v1,kind Deployment,metadata{name web-app,namespace production,labels{app web,tier frontend}},spec{replicas 3,selector{matchLabels{app web}},template{metadata{labels{app web}},spec{containers({name nginx,image nginx:1.25,ports({containerPort 80}),env({name API_URL,value https://api.example.com}{name DB_PASSWORD,valueFrom{secretKeyRef{name db-secrets,key password}}}),resources{limits{cpu 500m,memory 256Mi},requests{cpu 100m,memory 128Mi}},volumeMounts({name config,mountPath /etc/nginx/conf.d,readOnly true})}),volumes({name config,configMap{name nginx-config}})}}}}
```

## Observations

Comparing the YAML and STYX versions:

| Aspect | YAML | STYX |
|--------|------|------|
| Lines | 47 | 36 |
| Quoting | Required for some strings | Rarely needed |
| Lists | `- item` syntax | `(item item)` syntax |
| Nesting | Indentation-based | Explicit braces |
| Inline objects | Awkward `{key: value}` | Natural `{ key value }` |
| Single-line possible | No | Yes (minified) |
