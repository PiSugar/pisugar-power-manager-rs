# HTTP API

## Authentication
To enable authentication for the PiSugar Server, you can set the username and password during installation or configuration, or by directly editing the configuration file located at `/etc/pisugar-server/config.json`.

```json
{
  "auth_user": "your_username",
  "auth_password": "your_password"
}
```

You need to login with the specified username and password to access the server's HTTP interface. If authentication is not set up, the server will allow access without requiring credentials.


```txt
/login?username=your_username&password=your_password 
```

A jwt token will be returned for further authenticated requests.

Then, to access the protected resources, include the token in the `x-pisugar-token: <token>` header of the query `?token=<token>`:


## Execute Command

```
/exec?cmd=your_command&token=your_token
```
