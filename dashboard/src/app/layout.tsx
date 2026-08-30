import type { Metadata } from "next";
import "./globals.css";
import { Shell } from "@/components/shell";
import { ThemeProvider } from "@/lib/theme";

export const metadata: Metadata = {
  title: "GPUMesh",
  description: "Local console for your P2P GPU network",
};

const themeBoot = `(function(){try{var t=localStorage.getItem('gpumesh-theme');var d=t==='dark'||(t!=='light'&&window.matchMedia('(prefers-color-scheme: dark)').matches);if(d)document.documentElement.classList.add('dark');}catch(e){}})();`;

export default function RootLayout({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="en" suppressHydrationWarning>
      <head>
        <script dangerouslySetInnerHTML={{ __html: themeBoot }} />
      </head>
      <body className="antialiased">
        <ThemeProvider>
          <Shell>{children}</Shell>
        </ThemeProvider>
      </body>
    </html>
  );
}
