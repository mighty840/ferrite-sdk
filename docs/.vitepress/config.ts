import { defineConfig } from 'vitepress'

export default defineConfig({
  title: 'ferrite-sdk',
  description: 'Embedded Rust firmware observability SDK for Cortex-M devices',
  base: '/ferrite-sdk/',

  head: [
    ['link', { rel: 'icon', href: '/ferrite-sdk/logo.svg' }],
  ],

  themeConfig: {
    logo: '/logo.svg',

    nav: [
      { text: 'Guide', link: '/guide/introduction' },
      { text: 'Server', link: '/server/' },
      { text: 'Dashboard', link: '/dashboard/' },
      { text: 'Reference', link: '/reference/' },
      { text: 'Changelog', link: 'https://github.com/mighty840/ferrite-sdk/releases' },
    ],

    sidebar: {
      '/guide/': [
        {
          text: 'Getting Started',
          items: [
            { text: 'Introduction', link: '/guide/introduction' },
            { text: 'Quickstart', link: '/guide/quickstart' },
            { text: 'Core Concepts', link: '/guide/concepts' },
            { text: 'Architecture', link: '/guide/architecture' },
          ],
        },
        {
          text: 'Advanced',
          items: [
            { text: 'Transport Layer', link: '/guide/transports' },
            { text: 'Security & Encryption', link: '/guide/security' },
          ],
        },
      ],
      '/integrations/': [
        {
          text: 'Integrations',
          items: [
            { text: 'Overview', link: '/integrations/' },
            { text: 'Embassy', link: '/integrations/embassy' },
            { text: 'RTIC', link: '/integrations/rtic' },
            { text: 'Bare-metal', link: '/integrations/baremetal' },
            { text: 'Zephyr (C)', link: '/integrations/zephyr-c' },
            { text: 'FreeRTOS (C)', link: '/integrations/freertos-c' },
          ],
        },
      ],
      '/targets/': [
        {
          text: 'Target Platforms',
          items: [
            { text: 'Overview', link: '/targets/' },
            { text: 'nRF52840', link: '/targets/nrf52840' },
            { text: 'RP2040', link: '/targets/rp2040' },
            { text: 'STM32F4', link: '/targets/stm32f4' },
          ],
        },
      ],
      '/reference/': [
        {
          text: 'API Reference',
          items: [
            { text: 'Overview', link: '/reference/' },
            { text: 'Rust SDK API', link: '/reference/sdk-api' },
            { text: 'C FFI API', link: '/reference/c-api' },
            { text: 'Chunk Wire Format', link: '/reference/chunk-format' },
            { text: 'SdkConfig', link: '/reference/config' },
          ],
        },
      ],
      '/server/': [
        {
          text: 'Server',
          items: [
            { text: 'Overview', link: '/server/' },
            { text: 'Installation', link: '/server/installation' },
            { text: 'Configuration', link: '/server/configuration' },
            { text: 'Symbolication', link: '/server/symbolication' },
          ],
        },
      ],
      '/dashboard/': [
        {
          text: 'Dashboard',
          items: [
            { text: 'Overview', link: '/dashboard/' },
            { text: 'SSO / Keycloak', link: '/dashboard/sso' },
          ],
        },
      ],
      '/gateway/': [
        {
          text: 'Edge Gateway',
          items: [
            { text: 'Overview', link: '/gateway/' },
          ],
        },
      ],
      '/contributing/': [
        {
          text: 'Contributing',
          items: [
            { text: 'Overview', link: '/contributing/' },
            { text: 'Testing', link: '/contributing/testing' },
          ],
        },
      ],
    },

    editLink: {
      pattern: 'https://github.com/mighty840/ferrite-sdk/edit/main/docs/:path',
      text: 'Edit this page on GitHub',
    },

    search: {
      provider: 'local',
    },

    socialLinks: [
      { icon: 'github', link: 'https://github.com/mighty840/ferrite-sdk' },
    ],

    footer: {
      message: 'Released under the MIT License. <a href="/ferrite-sdk/legal/privacy-policy">Privacy Policy</a> · <a href="/ferrite-sdk/legal/impressum">Impressum</a>',
      copyright: 'Copyright 2024-present ferrite contributors',
    },
  },
})
