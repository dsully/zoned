terraform {
  required_providers {
    gandi = {
      version = "~> 2.0.0"
      source  = "go-gandi/gandi"
    }
  }
}

provider "gandi" {
  key = ""
}

resource "gandi_livedns_record" "sully_org" {
  for_each = jsondecode(file("${path.module}/sully.org.json"))

  zone = "sully.org"

  name   = each.value.name
  ttl    = each.value.ttl
  type   = each.value.type
  values = each.value.values
}
