// Minimal: start sharing bandwidth in the background (Node).
//   npm i @meerkly/sdk   # (built via scripts/build-node-addon.sh)
//   node start.js
const { ProxyClient } = require("@meerkly/sdk");

(async () => {
  const client = new ProxyClient({
    publisherId: process.env.MEERKLY_PUBLISHER_ID || "your-publisher-id",
    gatewayAddresses: ["127.0.0.1:4443"], // dev override (prod bakes this in)
    caCertPath: "certs/dev/ca.crt",        // dev override
  });

  await client.start(); // connects + registers; the exit node now runs in the background
  console.log("sharing bandwidth as", client.clientKey);

  // Your app keeps running normally; the proxy works in the background.
  process.on("SIGINT", () => client.stop().finally(() => process.exit(0)));
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
