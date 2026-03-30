import { defineConfig } from 'vitepress'

export default defineConfig({
  title: 'Cheeseboard',
  description: 'Cross-device clipboard sync over Tailscale',
  base: '/cheeseboard/',

  head: [
    ['link', { rel: 'icon', type: 'image/png', href: '/cheeseboard/cheese.png' }],
  ],

  themeConfig: {
    nav: [
      { text: 'Guide', link: '/guide/getting-started' },
      { text: 'Download', link: 'https://github.com/jamesyong-42/cheeseboard/releases' },
      { text: 'GitHub', link: 'https://github.com/jamesyong-42/cheeseboard' },
    ],

    sidebar: [
      {
        text: 'Guide',
        items: [
          { text: 'Getting Started', link: '/guide/getting-started' },
          { text: 'How It Works', link: '/guide/how-it-works' },
          { text: 'Architecture', link: '/guide/architecture' },
          { text: 'Building from Source', link: '/guide/building' },
        ],
      },
    ],

    socialLinks: [
      { icon: 'github', link: 'https://github.com/jamesyong-42/cheeseboard' },
    ],

    footer: {
      message: 'Released under the MIT License.',
      copyright: 'Copyright 2026 James Yong',
    },
  },
})
