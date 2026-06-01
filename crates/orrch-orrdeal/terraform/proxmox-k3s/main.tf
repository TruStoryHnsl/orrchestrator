terraform {
  required_providers {
    proxmox = {
      source  = "bpg/proxmox"
      version = "~> 0.66"
    }
  }
}

provider "proxmox" {
  endpoint  = var.pve_endpoint
  api_token = var.pve_api_token
  insecure  = true
}

# Single-node k3s VM cloned from a cloud-init-enabled template.
# NOTE: var.template must be the numeric VM id of a cloud-init template that has
# the qemu-guest-agent installed (so ipv4_addresses is populated after boot).
resource "proxmox_virtual_environment_vm" "k3s" {
  name      = var.vm_name
  node_name = var.pve_node

  clone {
    vm_id = tonumber(var.template)
  }

  cpu {
    cores = var.cores
  }

  memory {
    dedicated = var.memory_mb
  }

  agent {
    enabled = true
  }

  initialization {
    ip_config {
      ipv4 {
        address = "dhcp"
      }
    }
    user_account {
      username = var.ssh_user
      keys     = [trimspace(file(pathexpand(var.ssh_public_key_path)))]
    }
  }
}

# After the VM has an IP, install k3s and expose a readable, IP-localized kubeconfig.
resource "null_resource" "kubeconfig" {
  depends_on = [proxmox_virtual_environment_vm.k3s]

  connection {
    type        = "ssh"
    host        = proxmox_virtual_environment_vm.k3s.ipv4_addresses[1][0]
    user        = var.ssh_user
    private_key = file(pathexpand(var.ssh_private_key_path))
    timeout     = "5m"
  }

  provisioner "remote-exec" {
    inline = [
      "curl -sfL https://get.k3s.io | sudo sh -",
      "sudo install -m 644 /etc/rancher/k3s/k3s.yaml /home/${var.ssh_user}/k3s.yaml",
      "sudo chown ${var.ssh_user} /home/${var.ssh_user}/k3s.yaml",
    ]
  }

  provisioner "local-exec" {
    command = <<-EOT
      scp -o StrictHostKeyChecking=accept-new -i ${pathexpand(var.ssh_private_key_path)} ${var.ssh_user}@${proxmox_virtual_environment_vm.k3s.ipv4_addresses[1][0]}:k3s.yaml ${path.module}/kubeconfig.yaml
      sed -i 's#https://127.0.0.1:6443#https://${proxmox_virtual_environment_vm.k3s.ipv4_addresses[1][0]}:6443#' ${path.module}/kubeconfig.yaml
    EOT
  }
}
