import { PageHeader } from "@/components/ui";

export default function SecurityPage() {
  return (
    <div>
      <PageHeader
        title="Security"
        subtitle="How GPUMesh keeps remote access sandboxed."
      />
      <div className="panel space-y-5 p-6 text-sm leading-relaxed text-mist-300">
        <p>
          Remote users never receive an unrestricted host shell. The path is
          always:
        </p>
        <pre className="code-block">
{`Remote user
  → authenticated P2P (Ed25519 + QUIC)
  → job sandbox
  → Docker container
  → NVIDIA GPU`}
        </pre>
        <ul className="list-disc space-y-2 pl-5">
          <li>Default-deny allowlist on providers</li>
          <li>Mutual pairing for private clusters</li>
          <li>Control plane never runs GPU containers — it dials peers only</li>
          <li>
            Dashboard connect/run uses an ephemeral QUIC dialer (won’t steal the
            share port)
          </li>
          <li>Resource limits: VRAM, concurrency, runtime</li>
        </ul>
      </div>
    </div>
  );
}
