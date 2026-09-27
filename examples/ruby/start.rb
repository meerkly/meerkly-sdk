# Minimal: start sharing bandwidth in the background (Ruby).
#   RUBYLIB=sdk/ruby DYLD_LIBRARY_PATH=sdk/ruby ruby examples/ruby/start.rb
#
# uniffi's Ruby backend has no async support, so Ruby uses the blocking
# start/stop variants (they still return as soon as the client is connected).
require "meerkly"

client = Meerkly::ProxyClient.new(Meerkly::ProxyConfig.new(
  publisher_id: ENV["MEERKLY_PUBLISHER_ID"] || "your-publisher-id",
  gateway_addresses: ["127.0.0.1:4443"], # dev override (prod bakes this in)
  ca_cert_path: "certs/dev/ca.crt",       # dev override
))

client.start_blocking # connects + registers; the exit node now runs in the background
puts "sharing bandwidth as #{client.client_key}"

# Keep the program alive; the proxy works in the background.
trap("INT") { client.stop_blocking; exit }
sleep
