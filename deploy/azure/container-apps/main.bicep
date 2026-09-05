targetScope = 'resourceGroup'

@description('Azure region')
param location string = resourceGroup().location

@allowed([
  'cost-optimized'
  'high-availability'
])
param profile string = 'high-availability'

param image string = 'ghcr.io/dlamaro96/inferqos:0.1.0'

@secure()
param configYaml string

param internalOnly bool = true

resource identity 'Microsoft.ManagedIdentity/userAssignedIdentities@2023-01-31' = {
  name: 'inferqos'
  location: location
}

resource logs 'Microsoft.OperationalInsights/workspaces@2023-09-01' = {
  name: 'inferqos-logs'
  location: location
  properties: {
    retentionInDays: 30
  }
}

resource environment 'Microsoft.App/managedEnvironments@2024-03-01' = {
  name: 'inferqos-env'
  location: location
  properties: {
    appLogsConfiguration: {
      destination: 'log-analytics'
      logAnalyticsConfiguration: {
        customerId: logs.properties.customerId
        sharedKey: logs.listKeys().primarySharedKey
      }
    }
  }
}

resource app 'Microsoft.App/containerApps@2024-03-01' = {
  name: 'inferqos'
  location: location
  identity: {
    type: 'UserAssigned'
    userAssignedIdentities: {
      '${identity.id}': {}
    }
  }
  properties: {
    managedEnvironmentId: environment.id
    configuration: {
      ingress: {
        external: !internalOnly
        targetPort: 8080
        transport: 'auto'
        allowInsecure: false
      }
      secrets: [
        {
          name: 'config'
          value: configYaml
        }
      ]
    }
    template: {
      containers: [
        {
          name: 'inferqos'
          image: image
          args: [
            'serve'
            '--config'
            '/mnt/config/inferqos.yaml'
          ]
          resources: {
            cpu: json(profile == 'high-availability' ? '0.5' : '0.25')
            memory: profile == 'high-availability' ? '1Gi' : '0.5Gi'
          }
          volumeMounts: [
            {
              volumeName: 'config'
              mountPath: '/mnt/config'
            }
          ]
        }
      ]
      volumes: [
        {
          name: 'config'
          storageType: 'Secret'
          secrets: [
            {
              secretRef: 'config'
              path: 'inferqos.yaml'
            }
          ]
        }
      ]
      scale: {
        minReplicas: profile == 'high-availability' ? 2 : 1
        maxReplicas: profile == 'high-availability' ? 10 : 3
        rules: [
          {
            name: 'http'
            http: {
              metadata: {
                concurrentRequests: '50'
              }
            }
          }
        ]
      }
    }
  }
}

output endpoint string = 'https://${app.properties.configuration.ingress.fqdn}'
