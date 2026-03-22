using System;
using System.Globalization;
using System.Windows;
using System.Windows.Data;

namespace Yap.UI
{
    /// <summary>
    /// Converts a text length (int) to Visibility.
    /// Returns Visible when length is 0 (show watermark), Collapsed otherwise (hide watermark).
    /// </summary>
    public class WatermarkVisibilityConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
        {
            if (value is int length)
                return length == 0 ? Visibility.Visible : Visibility.Collapsed;
            return Visibility.Collapsed;
        }

        public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
        {
            throw new NotImplementedException();
        }
    }
}
