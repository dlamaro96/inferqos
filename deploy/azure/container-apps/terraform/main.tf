terraform { required_version = ">= 1.6.0"; required_providers { azurerm = { source = "hashicorp/azurerm"; version = "~> 4.0" } } }
provider "azurerm" { features {} }
variable "resource_group_name" { type = string }
variable "location" { type = string }
variable "config_yaml" { type = string; sensitive = true }
variable "image" { type = string; default = "ghcr.io/dlamaro96/inferqos:0.1.0" }
resource "azurerm_log_analytics_workspace" "this" { name = "inferqos-logs"; location = var.location; resource_group_name = var.resource_group_name; sku = "PerGB2018"; retention_in_days = 30 }
resource "azurerm_container_app_environment" "this" { name = "inferqos-env"; location = var.location; resource_group_name = var.resource_group_name; log_analytics_workspace_id = azurerm_log_analytics_workspace.this.id }
resource "azurerm_user_assigned_identity" "this" { name = "inferqos"; location = var.location; resource_group_name = var.resource_group_name }
resource "azurerm_container_app" "this" {
  name = "inferqos"; resource_group_name = var.resource_group_name; container_app_environment_id = azurerm_container_app_environment.this.id; revision_mode = "Single"
  identity { type = "UserAssigned"; identity_ids = [azurerm_user_assigned_identity.this.id] }
  secret { name = "config"; value = var.config_yaml }
  template { min_replicas = 2; max_replicas = 10; container { name = "inferqos"; image = var.image; cpu = 0.5; memory = "1Gi"; args = ["serve","--config","/mnt/config/inferqos.yaml"] } }
  ingress { external_enabled = false; target_port = 8080; transport = "auto"; traffic_weight { percentage = 100; latest_revision = true } }
}

