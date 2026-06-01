variable "pve_endpoint" {
  type = string
}
variable "pve_node" {
  type = string
}
variable "pve_api_token" {
  type      = string
  sensitive = true
}
variable "template" {
  type = string
}
variable "vm_name" {
  type    = string
  default = "orrdeal-skeleton"
}
variable "cores" {
  type    = number
  default = 2
}
variable "memory_mb" {
  type    = number
  default = 2048
}
variable "ssh_user" {
  type = string
}
variable "ssh_public_key_path" {
  type = string
}
variable "ssh_private_key_path" {
  type = string
}
