backend "event_bus" "events" {}

fault "slow" {
  phase = "before_response"
  delay { milliseconds = 10 }
}

fault_profile "seeded" {
  probability = 0.5
  faults = ["slow"]
  operations = ["limits"]
  seed = 7
}

async "publish_order" {
  action "event_bus.events" "publish" {
    topic = "orders.created"
    body  = response
  }
}

effect "publish_created_order" {
  operations = ["createOrder"]
  async = ["publish_order"]
}

webhook "order.created" {}
webhook "order.ready" {}

effect "announce_order" {
  operations = ["createOrder"]
  emit = {
    "order.created" = response
    "order.ready" = response
  }
  after = { "order.ready" = ["order.created"] }
  to = {
    "order.created" = "https://example.test/hook"
    "order.ready" = "https://example.test/hook"
  }
}
