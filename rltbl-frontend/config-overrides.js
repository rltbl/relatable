const webpack = require('webpack');

module.exports = {
    // The Webpack config to use when compiling your react app for development or production.
    webpack: function (config, env) {
        // ...add your webpack config
        config.output.filename = "static/js/[name].js";
        // https://github.com/facebook/create-react-app/issues/5306#issuecomment-1426637113
        config.plugins = [
          ...config.plugins,
          new webpack.optimize.LimitChunkCountPlugin({
            maxChunks: 1,
          }),
        ];
        return config;
    }
}
