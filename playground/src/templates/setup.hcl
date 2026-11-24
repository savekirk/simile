principal "application" {
  description = "An integration permitted to manage orders."
  credential "apiKey" {}
}
principal_setup "application" {
  operation = "createApplication"
  collect "name" {
    description = "Integration name"
    required = true
    to = request.name
  }
  issue "api_key" {
    generate = "token"
    prefix = "sk_simile_"
    credential = "apiKey"
    sensitive = true
  }
}
