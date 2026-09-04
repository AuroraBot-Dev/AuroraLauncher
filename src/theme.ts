import type { GlobalThemeOverrides } from 'naive-ui'

export const darkThemeOverrides: GlobalThemeOverrides = {
  common: {
    primaryColor: '#17c3c8',
    primaryColorHover: '#24d8d2',
    primaryColorPressed: '#0ba7ad',
    primaryColorSuppl: '#17c3c8',
    infoColor: '#7aa7ff',
    infoColorHover: '#8db4ff',
    successColor: '#3fcf8e',
    successColorHover: '#52dd9d',
    successColorPressed: '#2eb97a',
    warningColor: '#f5b64b',
    warningColorHover: '#f7c463',
    warningColorPressed: '#dca232',
    errorColor: '#ff6b7a',
    errorColorHover: '#ff8290',
    errorColorPressed: '#e75564',
    bodyColor: '#0a0f16',
    cardColor: '#111a24',
    modalColor: '#131d27',
    popoverColor: '#17232d',
    tableColor: '#111a24',
    actionColor: '#15202c',
    borderColor: '#22303c',
    dividerColor: '#22303c',
    borderRadius: '7px',
    borderRadiusSmall: '6px',
    textColorBase: '#d8e2ea',
    textColor1: '#e6edf3',
    textColor2: '#c3cfd9',
    textColor3: '#8a99a6',
    textColorDisabled: '#566570',
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
    railColor: '#1a2934',
    textColor: '#8a99a6'
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
