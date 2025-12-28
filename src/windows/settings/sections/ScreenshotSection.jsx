import { useTranslation } from 'react-i18next';
import { useEffect, useMemo, useState } from 'react';
import SettingsSection from '../components/SettingsSection';
import SettingItem from '../components/SettingItem';
import Toggle from '@shared/components/ui/Toggle';
import Select from '@shared/components/ui/Select';
import { isWindows } from '@shared/utils/platform';
function ScreenshotSection({
  settings,
  onSettingChange
}) {
  const {
    t
  } = useTranslation();
  const [isWindowsPlatform, setIsWindowsPlatform] = useState(false);

  useEffect(() => {
    isWindows().then(setIsWindowsPlatform).catch(() => setIsWindowsPlatform(false));
  }, []);

  const elementDetectionOptions = useMemo(() => {
    const allOptions = [{
      value: 'none',
      label: t('settings.screenshot.detectionNone')
    }, {
      value: 'window',
      label: t('settings.screenshot.detectionWindow')
    }, {
      value: 'all',
      label: t('settings.screenshot.detectionAll')
    }];
    return isWindowsPlatform ? allOptions : allOptions.slice(0, 1);
  }, [isWindowsPlatform, t]);
  return <SettingsSection title={t('settings.screenshot.title')} description={t('settings.screenshot.description')}>
      <SettingItem label={t('settings.screenshot.enabled')} description={t('settings.screenshot.enabledDesc')}>
        <Toggle checked={settings.screenshotEnabled} onChange={checked => onSettingChange('screenshotEnabled', checked)} />
      </SettingItem>

      <SettingItem label={t('settings.screenshot.elementDetection')} description={t('settings.screenshot.elementDetectionDesc')}>
        <Select
          value={isWindowsPlatform ? (settings.screenshotElementDetection || 'all') : 'none'}
          onChange={value => onSettingChange('screenshotElementDetection', value)}
          options={elementDetectionOptions}
          className="w-48"
          disabled={!isWindowsPlatform}
        />
      </SettingItem>

      <SettingItem label={t('settings.screenshot.magnifier')} description={t('settings.screenshot.magnifierDesc')}>
        <Toggle checked={settings.screenshotMagnifierEnabled} onChange={checked => onSettingChange('screenshotMagnifierEnabled', checked)} />
      </SettingItem>

      <SettingItem label={t('settings.screenshot.hints')} description={t('settings.screenshot.hintsDesc')}>
        <Toggle checked={settings.screenshotHintsEnabled} onChange={checked => onSettingChange('screenshotHintsEnabled', checked)} />
      </SettingItem>

      <SettingItem label={t('settings.screenshot.colorIncludeFormat')} description={t('settings.screenshot.colorIncludeFormatDesc')}>
        <Toggle checked={settings.screenshotColorIncludeFormat} onChange={checked => onSettingChange('screenshotColorIncludeFormat', checked)} />
      </SettingItem>
    </SettingsSection>;
}
export default ScreenshotSection;
