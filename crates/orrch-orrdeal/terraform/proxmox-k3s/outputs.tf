output "node_ipv4" {
  value = proxmox_virtual_environment_vm.k3s.ipv4_addresses[1][0]
}

output "kubeconfig_path" {
  value = "${path.module}/kubeconfig.yaml"
}
