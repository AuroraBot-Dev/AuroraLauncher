import type { GlobalThemeOverrides } from 'naive-ui'

export const darkThemeOverrides: GlobalThemeOverrides = {
  common: {
    primaryColor: '#006be6',
    primaryColorHover: '#0a7cff',
    primaryColorPressed: '#005ac2',
    primaryColorSuppl: '#006be6',
    infoColor: '#006be6',
    infoColorHover: '#0a7cff',
    infoColorPressed: '#005ac2',
    successColor: '#22c55e',
    successColorHover: '#34d06e',
    successColorPressed: '#16a34a',
    warningColor: '#f59e0b',
    warningColorHover: '#fbbf24',
    warningColorPressed: '#d97706',
    errorColor: '#ff3b5c',
    errorColorHover: '#ff5573',
    errorColorPressed: '#e0294a',
    bodyColor: '#1c1e23',
    cardColor: '#1c1e23',
    modalColor: '#1c1e23',
    popoverColor: '#242424',
    tableColor: '#1c1e23',
    actionColor: '#2e3138',
    borderColor: '#36363a',
    dividerColor: '#2a2a2e',
    borderRadius: '7px',
    borderRadiusSmall: '6px',
    // 亮色元素（主按钮/主色标签）上的文字用白色，避免暗色主题默认翻成黑字
    baseColor: '#fafafa',
    textColorBase: '#f2f2f2',
    textColor1: '#fafafa',
    textColor2: '#d4d4d8',
    textColor3: '#a1a1aa',
    textColorDisabled: '#6b6b70',
    fontFamily:
      "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, 'PingFang SC', 'Hiragino Sans GB', 'Microsoft YaHei', sans-serif",
    fontFamilyMono: "'JetBrains Mono', 'Cascadia Code', 'SFMono-Regular', Consolas, monospace"
  },
  Button: {
    fontWeight: '500',
    borderRadiusMedium: '7px'
  },
  Tag: {
    borderRadius: '5px'
  },
  Progress: {
    railColor: '#2a2a2e',
    textColor: '#a1a1aa'
  },
  Message: {
    borderRadius: '8px'
  }
}

export const lightThemeOverrides: GlobalThemeOverrides = {
  common: {
    primaryColor: '#0f9fa7',
    primaryColorHover: '#0fb2b9',
    primaryColorPressed: '#0d8b92',
    primaryColorSuppl: '#0f9fa7',
    infoColor: '#2f6fd6',
    infoColorHover: '#4784e4',
    successColor: '#1aa268',
    successColorHover: '#26b677',
    successColorPressed: '#158c59',
    warningColor: '#c18416',
    warningColorHover: '#d5972a',
    warningColorPressed: '#a66d0f',
    errorColor: '#dc4a57',
    errorColorHover: '#e65b69',
    errorColorPressed: '#c43a48',
    bodyColor: '#f3f6f8',
    cardColor: '#ffffff',
    modalColor: '#ffffff',
    popoverColor: '#ffffff',
    tableColor: '#ffffff',
    actionColor: '#f7f9fa',
    borderColor: '#dfe6ea',
    dividerColor: '#e5ebee',
    borderRadius: '7px',
    borderRadiusSmall: '6px',
    textColorBase: '#1c2a33',
    textColor1: '#17242d',
    textColor2: '#3e535f',
    textColor3: '#8497a4',
    textColorDisabled: '#b3c0c8'
  },
  Button: {
    fontWeight: '500',
    borderRadiusMedium: '7px'
  },
  Tag: {
    borderRadius: '5px'
  },
  Progress: {
    railColor: '#e4ebef',
    textColor: '#3e535f'
  }
}
