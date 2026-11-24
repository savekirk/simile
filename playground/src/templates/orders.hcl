backend "db" "primary" {
  description = "Order records exposed by the public API."
}
backend "event_bus" "events" {}

async "publish_order" {
  action "event_bus.events" "publish" {
    topic = "orders.created"
    body = response
  }
}
effect "announce_order" {
  operations = ["createOrder"]
  action "db.primary" "put" {
    collection = "orders"
    key = response.id
    value = response
  }
  async = ["publish_order"]
  emit = { "order.created" = response }
  to = { "order.created" = "https://example.test/orders-hook" }
}
fault "slow" {
  phase = "before_response"
  delay { milliseconds = 250 }
}
fault_profile "latency" {
  probability = 1
  faults = ["slow"]
  operations = ["createOrder"]
  seed = 7
}
