import type { NextConfig } from "next";

const api = process.env.GPUMESH_API_INTERNAL ?? "http://127.0.0.1:8080";

const nextConfig: NextConfig = {
  reactStrictMode: true,
  async rewrites() {
    return [
      {
        source: "/gpumesh/:path*",
        destination: `${api}/:path*`,
      },
    ];
  },
};

export default nextConfig;
